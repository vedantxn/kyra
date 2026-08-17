use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use tokio::time::{timeout, Duration};
use url::Url;

use super::types::{
    AiProvider, InferenceRequest, OllamaModel, ProviderConfig, ProviderHealth, ProviderInference,
    ProviderUsage,
};

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("The provider configuration is incomplete.")]
    Configuration,
    #[error("Ollama must use a loopback URL such as http://127.0.0.1:11434.")]
    UnsafeOllamaUrl,
    #[error("The model provider rejected these credentials.")]
    Authentication,
    #[error("The model provider is temporarily rate limiting Kyra.")]
    RateLimited { retry_after_seconds: Option<u64> },
    #[error("The model provider is temporarily unavailable.")]
    Unavailable,
    #[error("The model did not respond within 90 seconds.")]
    Timeout,
    #[error("The model refused the request.")]
    Refusal,
    #[error("The model returned invalid structured output.")]
    InvalidOutput,
    #[error("The configured model was not found.")]
    ModelNotFound,
}

impl ProviderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::UnsafeOllamaUrl => "unsafe_ollama_url",
            Self::Authentication => "authentication",
            Self::RateLimited { .. } => "rate_limited",
            Self::Unavailable => "unavailable",
            Self::Timeout => "timeout",
            Self::Refusal => "refusal",
            Self::InvalidOutput => "invalid_output",
            Self::ModelNotFound => "model_not_found",
        }
    }
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn provider(&self) -> AiProvider;
    fn requested_model(&self) -> &str;
    async fn health(&self) -> Result<ProviderHealth, ProviderError>;
    async fn discover_models(&self) -> Result<Vec<OllamaModel>, ProviderError> {
        Ok(Vec::new())
    }
    async fn infer(&self, request: InferenceRequest) -> Result<ProviderInference, ProviderError>;
}

pub fn create_provider(config: ProviderConfig) -> Result<Arc<dyn ModelProvider>, ProviderError> {
    let client = Client::builder()
        .user_agent("Kyra/0.1")
        .build()
        .map_err(|_| ProviderError::Configuration)?;
    create_provider_with_client(config, client, None)
}

fn create_provider_with_client(
    config: ProviderConfig,
    client: Client,
    endpoint_override: Option<String>,
) -> Result<Arc<dyn ModelProvider>, ProviderError> {
    if config.model.trim().is_empty() {
        return Err(ProviderError::Configuration);
    }
    match config.provider {
        AiProvider::Openai => Ok(Arc::new(OpenAiProvider {
            client,
            api_key: required_key(config.api_key)?,
            model: config.model,
            endpoint: endpoint_override.unwrap_or_else(|| "https://api.openai.com/v1".to_owned()),
        })),
        AiProvider::Anthropic => Ok(Arc::new(AnthropicProvider {
            client,
            api_key: required_key(config.api_key)?,
            model: config.model,
            endpoint: endpoint_override
                .unwrap_or_else(|| "https://api.anthropic.com/v1".to_owned()),
        })),
        AiProvider::Ollama => {
            let endpoint = endpoint_override
                .or(config.base_url)
                .unwrap_or_else(|| "http://127.0.0.1:11434".to_owned());
            validate_ollama_url(&endpoint)?;
            Ok(Arc::new(OllamaProvider {
                client,
                model: config.model,
                endpoint: endpoint.trim_end_matches('/').to_owned(),
            }))
        }
    }
}

fn required_key(key: Option<String>) -> Result<String, ProviderError> {
    key.map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or(ProviderError::Configuration)
}

pub fn validate_ollama_url(value: &str) -> Result<Url, ProviderError> {
    let url = Url::parse(value).map_err(|_| ProviderError::UnsafeOllamaUrl)?;
    let safe_host = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    if !safe_host || !matches!(url.scheme(), "http" | "https") || url.username() != "" {
        return Err(ProviderError::UnsafeOllamaUrl);
    }
    Ok(url)
}

struct OpenAiProvider {
    client: Client,
    api_key: String,
    model: String,
    endpoint: String,
}

#[async_trait]
impl ModelProvider for OpenAiProvider {
    fn provider(&self) -> AiProvider {
        AiProvider::Openai
    }

    fn requested_model(&self) -> &str {
        &self.model
    }

    async fn health(&self) -> Result<ProviderHealth, ProviderError> {
        let started = Instant::now();
        let response = self
            .client
            .get(format!("{}/models/{}", self.endpoint, self.model))
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|_| ProviderError::Unavailable)?;
        let response = checked(response)?;
        let payload: Value = response
            .json()
            .await
            .map_err(|_| ProviderError::InvalidOutput)?;
        Ok(ProviderHealth {
            provider: AiProvider::Openai,
            requested_model: self.model.clone(),
            resolved_model: payload["id"].as_str().unwrap_or(&self.model).to_owned(),
            model_digest: None,
            latency_ms: started.elapsed().as_millis() as i64,
        })
    }

    async fn infer(&self, request: InferenceRequest) -> Result<ProviderInference, ProviderError> {
        let payload = json!({
            "model": self.model,
            "store": false,
            "input": [
                {"role": "system", "content": [{"type": "input_text", "text": request.system_prompt}]},
                {"role": "user", "content": [{"type": "input_text", "text": request.document}]}
            ],
            "text": {"format": {"type": "json_schema", "name": "kyra_intent_envelope", "strict": true, "schema": request.schema}}
        });
        let started = Instant::now();
        let future = self
            .client
            .post(format!("{}/responses", self.endpoint))
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send();
        let response = timeout(Duration::from_secs(request.timeout_seconds), future)
            .await
            .map_err(|_| ProviderError::Timeout)?
            .map_err(|_| ProviderError::Unavailable)?;
        let response = checked(response)?;
        let payload: Value = response
            .json()
            .await
            .map_err(|_| ProviderError::InvalidOutput)?;
        if payload["output"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["type"] == "refusal"))
        {
            return Err(ProviderError::Refusal);
        }
        let content = payload["output"]
            .as_array()
            .and_then(|output| {
                output.iter().find_map(|message| {
                    message["content"].as_array().and_then(|content| {
                        content
                            .iter()
                            .find(|part| part["type"] == "output_text")
                            .and_then(|part| part["text"].as_str())
                    })
                })
            })
            .ok_or(ProviderError::InvalidOutput)?;
        parse_inference(
            content,
            payload["model"].as_str().unwrap_or(&self.model),
            ProviderUsage {
                input_units: payload["usage"]["input_tokens"].as_i64(),
                output_units: payload["usage"]["output_tokens"].as_i64(),
            },
            started,
        )
    }
}

struct AnthropicProvider {
    client: Client,
    api_key: String,
    model: String,
    endpoint: String,
}

#[async_trait]
impl ModelProvider for AnthropicProvider {
    fn provider(&self) -> AiProvider {
        AiProvider::Anthropic
    }

    fn requested_model(&self) -> &str {
        &self.model
    }

    async fn health(&self) -> Result<ProviderHealth, ProviderError> {
        Ok(ProviderHealth {
            provider: AiProvider::Anthropic,
            requested_model: self.model.clone(),
            resolved_model: self.model.clone(),
            model_digest: None,
            latency_ms: 0,
        })
    }

    async fn infer(&self, request: InferenceRequest) -> Result<ProviderInference, ProviderError> {
        let payload = json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": request.system_prompt,
            "messages": [{"role": "user", "content": request.document}],
            "output_config": {"format": {"type": "json_schema", "schema": request.schema}}
        });
        let started = Instant::now();
        let future = self
            .client
            .post(format!("{}/messages", self.endpoint))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&payload)
            .send();
        let response = timeout(Duration::from_secs(request.timeout_seconds), future)
            .await
            .map_err(|_| ProviderError::Timeout)?
            .map_err(|_| ProviderError::Unavailable)?;
        let response = checked(response)?;
        let payload: Value = response
            .json()
            .await
            .map_err(|_| ProviderError::InvalidOutput)?;
        if payload["stop_reason"] == "refusal" {
            return Err(ProviderError::Refusal);
        }
        let content = payload["content"]
            .as_array()
            .and_then(|parts| {
                parts
                    .iter()
                    .find(|part| part["type"] == "text")
                    .and_then(|part| part["text"].as_str())
            })
            .ok_or(ProviderError::InvalidOutput)?;
        parse_inference(
            content,
            payload["model"].as_str().unwrap_or(&self.model),
            ProviderUsage {
                input_units: payload["usage"]["input_tokens"].as_i64(),
                output_units: payload["usage"]["output_tokens"].as_i64(),
            },
            started,
        )
    }
}

struct OllamaProvider {
    client: Client,
    model: String,
    endpoint: String,
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    fn provider(&self) -> AiProvider {
        AiProvider::Ollama
    }

    fn requested_model(&self) -> &str {
        &self.model
    }

    async fn health(&self) -> Result<ProviderHealth, ProviderError> {
        let started = Instant::now();
        let models = self.discover_models().await?;
        let model = models
            .into_iter()
            .find(|model| model.name == self.model || model.name.starts_with(&self.model))
            .ok_or(ProviderError::ModelNotFound)?;
        Ok(ProviderHealth {
            provider: AiProvider::Ollama,
            requested_model: self.model.clone(),
            resolved_model: model.name,
            model_digest: Some(model.digest),
            latency_ms: started.elapsed().as_millis() as i64,
        })
    }

    async fn discover_models(&self) -> Result<Vec<OllamaModel>, ProviderError> {
        let response = self
            .client
            .get(format!("{}/api/tags", self.endpoint))
            .send()
            .await
            .map_err(|_| ProviderError::Unavailable)?;
        let payload: Value = checked(response)?
            .json()
            .await
            .map_err(|_| ProviderError::InvalidOutput)?;
        payload["models"]
            .as_array()
            .ok_or(ProviderError::InvalidOutput)?
            .iter()
            .map(|model| {
                Ok(OllamaModel {
                    name: model["name"]
                        .as_str()
                        .ok_or(ProviderError::InvalidOutput)?
                        .to_owned(),
                    digest: model["digest"]
                        .as_str()
                        .ok_or(ProviderError::InvalidOutput)?
                        .to_owned(),
                    size: model["size"].as_i64().unwrap_or_default(),
                })
            })
            .collect()
    }

    async fn infer(&self, request: InferenceRequest) -> Result<ProviderInference, ProviderError> {
        let payload = json!({
            "model": self.model,
            "stream": false,
            "messages": [
                {"role": "system", "content": request.system_prompt},
                {"role": "user", "content": request.document}
            ],
            "format": request.schema,
            "options": {"temperature": 0}
        });
        let started = Instant::now();
        let future = self
            .client
            .post(format!("{}/api/chat", self.endpoint))
            .json(&payload)
            .send();
        let response = timeout(Duration::from_secs(request.timeout_seconds), future)
            .await
            .map_err(|_| ProviderError::Timeout)?
            .map_err(|_| ProviderError::Unavailable)?;
        let payload: Value = checked(response)?
            .json()
            .await
            .map_err(|_| ProviderError::InvalidOutput)?;
        let content = payload["message"]["content"]
            .as_str()
            .ok_or(ProviderError::InvalidOutput)?;
        parse_inference(
            content,
            payload["model"].as_str().unwrap_or(&self.model),
            ProviderUsage {
                input_units: payload["prompt_eval_count"].as_i64(),
                output_units: payload["eval_count"].as_i64(),
            },
            started,
        )
    }
}

fn parse_inference(
    content: &str,
    resolved_model: &str,
    usage: ProviderUsage,
    started: Instant,
) -> Result<ProviderInference, ProviderError> {
    let envelope = serde_json::from_str(content).map_err(|_| ProviderError::InvalidOutput)?;
    Ok(ProviderInference {
        envelope,
        resolved_model: resolved_model.to_owned(),
        usage,
        latency_ms: started.elapsed().as_millis() as i64,
    })
}

fn checked(response: reqwest::Response) -> Result<reqwest::Response, ProviderError> {
    match response.status() {
        status if status.is_success() => Ok(response),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(ProviderError::Authentication),
        StatusCode::NOT_FOUND => Err(ProviderError::ModelNotFound),
        StatusCode::TOO_MANY_REQUESTS => {
            let retry_after_seconds = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok());
            Err(ProviderError::RateLimited {
                retry_after_seconds,
            })
        }
        status if status.is_server_error() => Err(ProviderError::Unavailable),
        _ => Err(ProviderError::InvalidOutput),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{contract::intent_json_schema, types::INTENT_SCHEMA_VERSION};
    use wiremock::{
        matchers::{body_json, header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn request(fingerprint: &str, _hash: &str) -> InferenceRequest {
        InferenceRequest {
            system_prompt: "Return proposals only.".to_owned(),
            document: "fixture".to_owned(),
            schema: intent_json_schema(),
            activation_fingerprint: fingerprint.to_owned(),
            timeout_seconds: 2,
        }
    }

    fn empty_envelope(fingerprint: &str, hash: &str) -> String {
        json!({
            "schemaVersion": INTENT_SCHEMA_VERSION,
            "activationFingerprint": fingerprint,
            "sourceDocumentHash": hash,
            "proposals": []
        })
        .to_string()
    }

    #[test]
    fn rejects_non_loopback_ollama_urls() {
        assert!(validate_ollama_url("http://127.0.0.1:11434").is_ok());
        assert!(validate_ollama_url("http://localhost:11434").is_ok());
        assert!(matches!(
            validate_ollama_url("https://ollama.example.com"),
            Err(ProviderError::UnsafeOllamaUrl)
        ));
    }

    #[tokio::test]
    async fn openai_uses_responses_strict_schema_and_store_false() {
        let server = MockServer::start().await;
        let expected = json!({
            "model": "gpt-test",
            "store": false,
            "input": [
                {"role": "system", "content": [{"type": "input_text", "text": "Return proposals only."}]},
                {"role": "user", "content": [{"type": "input_text", "text": "fixture"}]}
            ],
            "text": {"format": {"type": "json_schema", "name": "kyra_intent_envelope", "strict": true, "schema": intent_json_schema()}}
        });
        Mock::given(method("POST"))
            .and(path("/responses"))
            .and(header("authorization", "Bearer secret"))
            .and(body_json(expected))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model": "gpt-test-2026",
                "output": [{"type": "message", "content": [{"type": "output_text", "text": empty_envelope("fp", "hash")}]}],
                "usage": {"input_tokens": 5, "output_tokens": 8}
            })))
            .mount(&server)
            .await;
        let provider = create_provider_with_client(
            ProviderConfig {
                provider: AiProvider::Openai,
                model: "gpt-test".to_owned(),
                base_url: None,
                api_key: Some("secret".to_owned()),
            },
            Client::new(),
            Some(server.uri()),
        )
        .unwrap();
        let result = provider.infer(request("fp", "hash")).await.unwrap();
        assert_eq!(result.resolved_model, "gpt-test-2026");
        assert_eq!(result.envelope.source_document_hash, "hash");
    }

    #[tokio::test]
    async fn anthropic_and_ollama_translate_strict_requests() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model": "claude-test",
                "content": [{"type": "text", "text": empty_envelope("fp", "hash")}],
                "usage": {"input_tokens": 3, "output_tokens": 4}
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model": "llama-test",
                "message": {"content": empty_envelope("fp", "hash")},
                "prompt_eval_count": 2,
                "eval_count": 3
            })))
            .mount(&server)
            .await;

        let anthropic = create_provider_with_client(
            ProviderConfig {
                provider: AiProvider::Anthropic,
                model: "claude-test".to_owned(),
                base_url: None,
                api_key: Some("secret".to_owned()),
            },
            Client::new(),
            Some(server.uri()),
        )
        .unwrap();
        let ollama = create_provider_with_client(
            ProviderConfig {
                provider: AiProvider::Ollama,
                model: "llama-test".to_owned(),
                base_url: Some("http://127.0.0.1:11434".to_owned()),
                api_key: None,
            },
            Client::new(),
            Some(server.uri()),
        )
        .unwrap();
        assert!(anthropic.infer(request("fp", "hash")).await.is_ok());
        assert!(ollama.infer(request("fp", "hash")).await.is_ok());
    }
}
