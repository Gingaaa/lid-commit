use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use crate::config::Config;

/// Generate a commit message using the configured AI provider
pub async fn generate_commit_message(config: &Config, diff_summary: &str) -> Result<String> {
    match config.ai.provider.as_str() {
        "gemini" => gemini::generate(config, diff_summary).await,
        "openai" => openai::generate(config, diff_summary).await,
        "ollama" => ollama::generate(config, diff_summary).await,
        other => {
            warn!("Unknown AI provider '{}', using Gemini", other);
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
        let api_key = std::env::var(&config.ai.api_key_env).with_context(|| {
            format!(
                "Gemini API key not found. Set the '{}' environment variable.",
                config.ai.api_key_env
            )
        })?;

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
        let api_key = std::env::var(&config.ai.api_key_env)
            .context("OpenAI API key not found")?;

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
            model: config.ai.model.clone(),
            prompt,
            stream: false,
        };

        let client = reqwest::Client::new();
        let response = client
            .post("http://localhost:11434/api/generate")
            .json(&request)
            .send()
            .await
            .context("Failed to connect to Ollama. Is it running? `ollama serve`")?;

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
