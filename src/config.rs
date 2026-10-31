use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

/// Root configuration structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub ide: IdeConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub safety: SafetyConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeneralConfig {
    /// Push after committing
    pub push_after_commit: bool,
    /// Show OS notification on success
    pub notify_on_success: bool,
    /// Show OS notification on failure
    pub notify_on_failure: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            push_after_commit: true,
            notify_on_success: true,
            notify_on_failure: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiConfig {
    /// AI provider: "gemini", "openai", "claude", "ollama", "fallback"
    pub provider: String,
    /// Environment variable name that holds the API key
    pub api_key_env: String,
    /// Direct API key stored in config (optional, takes precedence if set)
    #[serde(default)]
    pub api_key: Option<String>,
    /// Ollama endpoint URL (default: http://localhost:11434)
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
    /// Model name
    pub model: String,
    /// Max lines of diff to send to AI (to avoid token limits)
    pub max_diff_lines: usize,
}

fn default_ollama_url() -> String {
    "http://localhost:11434".into()
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: "gemini".into(),
            api_key_env: "GEMINI_API_KEY".into(),
            api_key: None,
            ollama_url: default_ollama_url(),
            model: "gemini-2.0-flash".into(),
            max_diff_lines: 200,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IdeConfig {
    /// Which IDEs to watch: "vscode", "jetbrains", "any"
    pub watch: Vec<String>,
}

impl Default for IdeConfig {
    fn default() -> Self {
        Self {
            watch: vec!["vscode".into(), "jetbrains".into(), "any".into()],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitConfig {
    /// Prefix for auto-generated commit messages
    pub commit_prefix: String,
    /// Whether to push after committing
    pub push: bool,
    /// Paths to never auto-commit (glob patterns)
    pub excluded_paths: Vec<String>,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            commit_prefix: "🔒 lid-close:".into(),
            push: true,
            excluded_paths: vec![],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SafetyConfig {
    /// Skip if working tree has more than this many changed files
    pub max_files_changed: usize,
    /// Skip if there's no configured remote
    pub require_remote: bool,
    /// Skip if there are merge conflicts
    pub skip_on_conflicts: bool,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            max_files_changed: 50,
            require_remote: true,
            skip_on_conflicts: true,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            ai: AiConfig::default(),
            ide: IdeConfig::default(),
            git: GitConfig::default(),
            safety: SafetyConfig::default(),
        }
    }
}

/// Returns the path to the config file
fn config_path() -> PathBuf {
    if let Some(home) = dirs::config_dir() {
        home.join("lid-commit").join("config.toml")
    } else {
        PathBuf::from("config.toml")
    }
}

/// Load config from disk, falling back to defaults
pub async fn load() -> Result<Config> {
    let path = config_path();

    if !path.exists() {
        info!(
            "No config file found at {:?}, using defaults. \
             Run `lid-commit --init-config` to create one.",
            path
        );
        return Ok(Config::default());
    }

    let contents = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("Failed to read config file at {:?}", path))?;

    let config: Config = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse config file at {:?}", path))?;

    info!("Loaded config from {:?}", path);
    Ok(config)
}

/// Write the default config to disk (for --init-config)
pub async fn write_default() -> Result<PathBuf> {
    let path = config_path();

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let default_toml = include_str!("../config.example.toml");
    tokio::fs::write(&path, default_toml).await?;

    Ok(path)
}

/// Save config to disk (creates directories if necessary)
pub async fn save(config: &Config) -> Result<()> {
    let path = config_path();

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let toml_string = toml::to_string_pretty(config)
        .context("Failed to serialize configuration to TOML")?;

    tokio::fs::write(&path, toml_string)
        .await
        .with_context(|| format!("Failed to write configuration file at {:?}", path))?;

    info!("Configuration successfully saved to {:?}", path);
    Ok(())
}

