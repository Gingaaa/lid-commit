use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use crate::config::Config;

/// Generate a commit message using the configured AI provider
pub async fn generate_commit_message(config: &Config, diff_summary: &str) -> Result<String> {
    match config.ai.provider.to_lowercase().as_str() {
        "gemini" => gemini::generate(config, diff_summary).await,
        "openai" => openai::generate(config, diff_summary).await,
        "claude" | "anthropic" => claude::generate(config, diff_summary).await,
        "ollama" => ollama::generate(config, diff_summary).await,
        "fallback" | "offline" => {
            let now = chrono::Local::now();
            Ok(format!(
                "{} auto-save at {}",
                config.git.commit_prefix,
                now.format("%Y-%m-%d %H:%M")
            ))
        }
        other => {
            warn!("Unknown AI provider '{}', falling back to Gemini", other);
            gemini::generate(config, diff_summary).await
        }
    }
}

/// Build the prompt sent to the LLM
fn build_prompt(diff_summary: &str, commit_prefix: &str) -> String {
    format!(
        r#"You are an expert software developer writing git commit messages.

Your task: Write a concise, informative git commit message for the following code changes.

Rules:
- Use present tense (e.g., "add feature", not "added feature")
- Be specific — mention what changed and why if obvious
- Keep it under 72 characters
- Do NOT include the prefix "{commit_prefix}" — that will be added automatically
- Do NOT wrap in quotes
- Output ONLY the commit message, nothing else

Code changes:
{diff_summary}

Commit message:"#,
        commit_prefix = commit_prefix,
        diff_summary = diff_summary
    )
}

// ─────────────────────────────────────────────────────────────────
// Gemini API
// ─────────────────────────────────────────────────────────────────
mod gemini {
    use super::*;
    use serde::{Deserialize, Serialize};

    const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";

    #[derive(Serialize)]
    struct GeminiRequest {
        contents: Vec<Content>,
        #[serde(rename = "generationConfig")]
        generation_config: GenerationConfig,
    }

    #[derive(Serialize)]
    struct Content {
        parts: Vec<Part>,
    }

    #[derive(Serialize)]
    struct Part {
        text: String,
    }

    #[derive(Serialize)]
    struct GenerationConfig {
        temperature: f32,
        #[serde(rename = "maxOutputTokens")]
        max_output_tokens: u32,
    }

    #[derive(Deserialize)]
    struct GeminiResponse {
        candidates: Vec<Candidate>,
    }

    #[derive(Deserialize)]
    struct Candidate {
        content: ContentResponse,
    }

    #[derive(Deserialize)]
    struct ContentResponse {
        parts: Vec<PartResponse>,
    }

    #[derive(Deserialize)]
    struct PartResponse {
        text: String,
    }

    pub async fn generate(config: &Config, diff_summary: &str) -> Result<String> {
        let api_key = if let Some(ref key) = config.ai.api_key {
            if !key.trim().is_empty() {
                key.trim().to_string()
            } else {
                std::env::var(&config.ai.api_key_env).with_context(|| {
                    format!(
                        "Gemini API key not found in config or '{}' environment variable.",
                        config.ai.api_key_env
                    )
                })?
            }
        } else {
            std::env::var(&config.ai.api_key_env).with_context(|| {
                format!(
                    "Gemini API key not found in config or '{}' environment variable.",
                    config.ai.api_key_env
                )
            })?
        };

        let prompt = build_prompt(diff_summary, &config.git.commit_prefix);
        let url = format!(
            "{}/{}:generateContent?key={}",
            GEMINI_API_BASE, config.ai.model, api_key
        );

        let request = GeminiRequest {
            contents: vec![Content {
                parts: vec![Part { text: prompt }],
            }],
            generation_config: GenerationConfig {
                temperature: 0.3, // Low temperature for consistent, factual messages
                max_output_tokens: 100,
            },
        };

        debug!("Calling Gemini API with model: {}", config.ai.model);

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Gemini API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Gemini API returned {}: {}", status, body);
        }

        let gemini_resp: GeminiResponse = response
            .json()
            .await
            .context("Failed to parse Gemini API response")?;

        let raw_message = gemini_resp
            .candidates
            .into_iter()
            .next()
            .and_then(|c| c.content.parts.into_iter().next())
            .map(|p| p.text)
            .unwrap_or_else(|| "update code".to_string());

        // Clean up: remove surrounding quotes, trim whitespace, remove newlines
        let cleaned = raw_message
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .replace('\n', " ")
            .to_string();

        // Prepend the configured prefix
        let full_message = format!("{} {}", config.git.commit_prefix, cleaned);

        info!("AI generated commit message: {}", full_message);
        Ok(full_message)
    }
}

// ─────────────────────────────────────────────────────────────────
// OpenAI API (alternative)
// ─────────────────────────────────────────────────────────────────
mod openai {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    struct OpenAIRequest {
        model: String,
        messages: Vec<Message>,
        max_tokens: u32,
        temperature: f32,
    }

    #[derive(Serialize)]
    struct Message {
        role: String,
        content: String,
    }

    #[derive(Deserialize)]
    struct OpenAIResponse {
        choices: Vec<Choice>,
    }

    #[derive(Deserialize)]
    struct Choice {
        message: MessageResponse,
    }

    #[derive(Deserialize)]
    struct MessageResponse {
        content: String,
    }

    pub async fn generate(config: &Config, diff_summary: &str) -> Result<String> {
        let api_key = if let Some(ref key) = config.ai.api_key {
            if !key.trim().is_empty() {
                key.trim().to_string()
            } else {
                std::env::var(&config.ai.api_key_env)
                    .context("OpenAI API key not found in config or environment")?
            }
        } else {
            std::env::var(&config.ai.api_key_env)
                .context("OpenAI API key not found in config or environment")?
        };

        let prompt = build_prompt(diff_summary, &config.git.commit_prefix);

        let request = OpenAIRequest {
            model: config.ai.model.clone(),
            messages: vec![Message {
                role: "user".into(),
                content: prompt,
            }],
            max_tokens: 100,
            temperature: 0.3,
        };

        let client = reqwest::Client::new();
        let response = client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&request)
            .send()
            .await
            .context("Failed to call OpenAI API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API returned {}: {}", status, body);
        }

        let resp: OpenAIResponse = response.json().await?;
        let raw = resp
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_else(|| "update code".into());

        let cleaned = raw.trim().trim_matches('"').replace('\n', " ").to_string();
        Ok(format!("{} {}", config.git.commit_prefix, cleaned))
    }
}

// ─────────────────────────────────────────────────────────────────
// Anthropic Claude API (alternative)
// ─────────────────────────────────────────────────────────────────
mod claude {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    struct ClaudeRequest {
        model: String,
        messages: Vec<ClaudeMessage>,
        max_tokens: u32,
    }

    #[derive(Serialize)]
    struct ClaudeMessage {
        role: String,
        content: String,
    }

    #[derive(Deserialize)]
    struct ClaudeResponse {
        content: Vec<ClaudeContent>,
    }

    #[derive(Deserialize)]
    struct ClaudeContent {
        text: String,
    }

    pub async fn generate(config: &Config, diff_summary: &str) -> Result<String> {
        let api_key = if let Some(ref key) = config.ai.api_key {
            if !key.trim().is_empty() {
                key.trim().to_string()
            } else {
                std::env::var("ANTHROPIC_API_KEY")
                    .context("Anthropic API key not found in config or ANTHROPIC_API_KEY environment variable")?
            }
        } else {
            std::env::var("ANTHROPIC_API_KEY")
                .context("Anthropic API key not found in config or ANTHROPIC_API_KEY environment variable")?
        };

        let prompt = build_prompt(diff_summary, &config.git.commit_prefix);

        let request = ClaudeRequest {
            model: if config.ai.model.is_empty() || config.ai.model == "gemini-2.0-flash" {
                "claude-3-5-haiku-20241022".into()
            } else {
                config.ai.model.clone()
            },
            messages: vec![ClaudeMessage {
                role: "user".into(),
                content: prompt,
            }],
            max_tokens: 100,
        };

        let client = reqwest::Client::new();
        let response = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to call Anthropic Claude API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Claude API returned {}: {}", status, body);
        }

        let resp: ClaudeResponse = response.json().await?;
        let raw = resp
            .content
            .into_iter()
            .next()
            .map(|c| c.text)
            .unwrap_or_else(|| "update code".into());

        let cleaned = raw.trim().trim_matches('"').replace('\n', " ").to_string();
        Ok(format!("{} {}", config.git.commit_prefix, cleaned))
    }
}

// ─────────────────────────────────────────────────────────────────
// Ollama (local LLM, no API key required)
// ─────────────────────────────────────────────────────────────────
mod ollama {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    struct OllamaRequest {
        model: String,
        prompt: String,
        stream: bool,
    }

    #[derive(Deserialize)]
    struct OllamaResponse {
        response: String,
    }

    pub async fn generate(config: &Config, diff_summary: &str) -> Result<String> {
        let prompt = build_prompt(diff_summary, &config.git.commit_prefix);

        let request = OllamaRequest {
            model: if config.ai.model.is_empty() || config.ai.model == "gemini-2.0-flash" {
                "llama3.2".into()
            } else {
                config.ai.model.clone()
            },
            prompt,
            stream: false,
        };

        let mut base_url = config.ai.ollama_url.trim().trim_end_matches('/').to_string();
        if base_url.is_empty() {
            base_url = "http://localhost:11434".into();
        }
        let endpoint = format!("{}/api/generate", base_url);

        let client = reqwest::Client::new();
        let response = client
            .post(&endpoint)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("Failed to connect to Ollama at {}. Is it running? `ollama serve`", endpoint))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Ollama returned {}: {}", status, body);
        }

        let resp: OllamaResponse = response.json().await?;
        let cleaned = resp
            .response
            .trim()
            .trim_matches('"')
            .replace('\n', " ")
            .to_string();

        Ok(format!("{} {}", config.git.commit_prefix, cleaned))
    }
}
