use anyhow::{bail, Result};
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;
use tracing::warn;

/// Secret scanning patterns to block auto-committing exposed credentials
static SECRET_PATTERNS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    vec![
        // AWS Access Keys
        ("AWS Access Key ID", Regex::new(r#"(?i)\b(AKIA|ABIA|ACCA|ASIA)[0-9A-Z]{16}\b"#).unwrap()),
        // AWS Secret Keys
        ("AWS Secret Access Key", Regex::new(r#"(?i)aws[_\s]*secret[_\s]*access[_\s]*key\s*[:=]\s*['"][A-Za-z0-9/+=]{40}['"]"#).unwrap()),
        // Google / Gemini API Keys (starts with AIza, followed by ~35 alphanumeric/dash chars)
        ("Google / Gemini API Key", Regex::new(r#"\bAIza[0-9A-Za-z\-_]{30,45}\b"#).unwrap()),
        // GitHub Personal Access Tokens (Classic & Fine-grained)
        ("GitHub Personal Access Token", Regex::new(r#"\b(ghp_[0-9a-zA-Z]{36}|github_pat_[0-9a-zA-Z_]{82})\b"#).unwrap()),
        ("GitHub OAuth Access Token", Regex::new(r#"\bgho_[0-9a-zA-Z]{36}\b"#).unwrap()),
        // OpenAI API Keys (legacy sk- and project-scoped sk-proj-)
        ("OpenAI API Key", Regex::new(r#"\bsk-(proj-)?[a-zA-Z0-9\-_]{20,80}\b"#).unwrap()),
        // Anthropic / Claude API Keys
        ("Anthropic Claude API Key", Regex::new(r#"\bsk-ant-[a-zA-Z0-9\-_]{20,90}\b"#).unwrap()),
        // Slack Tokens & Webhook URLs
        ("Slack Token", Regex::new(r#"\bxox[baprs]-[0-9a-zA-Z]{10,48}\b"#).unwrap()),
        ("Slack Webhook URL", Regex::new(r#"https://hooks\.slack\.com/services/T[0-9a-zA-Z_]+/B[0-9a-zA-Z_]+/[0-9a-zA-Z_]+"#).unwrap()),
        // Generic Private RSA / DSA / EC / OpenSSH Keys
        ("Private Encryption Key", Regex::new(r#"-----BEGIN ((EC|RSA|DSA|OPENSSH) )?PRIVATE KEY-----"#).unwrap()),
        // High-entropy generic passwords / tokens assigned in assignments
        ("Generic Hardcoded Secret / Password", Regex::new(r#"(?i)(api[_-]?key|secret[_-]?key|auth[_-]?token|client[_-]?secret)\s*[:=]\s*['"][A-Za-z0-9\-_+=/]{16,64}['"]"#).unwrap()),
    ]
});

/// Dangerous / Sensitive file extensions and basenames that should NEVER be committed
const SENSITIVE_FILE_NAMES: &[&str] = &[
    ".env",
    ".env.local",
    ".env.production",
    ".env.development",
    ".env.staging",
    "id_rsa",
    "id_ed25519",
    "id_dsa",
    "id_ecdsa",
    "credentials.json",
    "service_account.json",
    "secrets.json",
    "secrets.toml",
    "secrets.yaml",
    "secrets.yml",
    "htpasswd",
];

const SENSITIVE_FILE_EXTENSIONS: &[&str] = &[
    "pem",
    "key",
    "pkcs12",
    "pfx",
    "p12",
    "kdbx",
    "sqlite",
    "sqlite3",
];

/// Result of a comprehensive pre-commit security audit
#[derive(Debug, Clone)]
pub struct SecurityAudit {
    pub is_safe: bool,
    pub violation_reason: Option<String>,
}

/// Perform a strict security audit on the repository before staging or committing
pub fn audit_repository(changed_files: &[&str], diff_content: &str) -> SecurityAudit {
    // 1. Audit changed file names & extensions
    for file in changed_files {
        let clean_path = file.trim_start_matches(['M', 'A', 'D', 'R', 'C', 'U', '?', ' ']).trim();
        let path_obj = Path::new(clean_path);

        // Check exact sensitive filename
        if let Some(file_name) = path_obj.file_name().and_then(|n| n.to_str()) {
            let lower_name = file_name.to_lowercase();
            for &sensitive in SENSITIVE_FILE_NAMES {
                if lower_name == sensitive || lower_name.starts_with(".env.") {
                    return SecurityAudit {
                        is_safe: false,
                        violation_reason: Some(format!(
                            "Security Guard: Blocked staging sensitive file '{}' to prevent secret exposure.",
                            file_name
                        )),
                    };
                }
            }

            // Check dangerous extension
            if let Some(ext) = path_obj.extension().and_then(|e| e.to_str()) {
                let lower_ext = ext.to_lowercase();
                if SENSITIVE_FILE_EXTENSIONS.contains(&lower_ext.as_str()) {
                    return SecurityAudit {
                        is_safe: false,
                        violation_reason: Some(format!(
                            "Security Guard: Blocked staging file with dangerous extension '.{}' ({}).",
                            lower_ext, file_name
                        )),
                    };
                }
            }
        }
    }

    // 2. Audit diff content for hardcoded API keys, JWTs, and private keys
    for (name, regex) in SECRET_PATTERNS.iter() {
        if let Some(matched) = regex.find(diff_content) {
            let matched_text = matched.as_str();
            let masked = if matched_text.len() > 8 {
                format!("{}...{}", &matched_text[..4], &matched_text[matched_text.len() - 4..])
            } else {
                "***".to_string()
            };

            warn!("⚠️ [Security Guard Alert] Detected potential secret ({}): {}", name, masked);

            return SecurityAudit {
                is_safe: false,
                violation_reason: Some(format!(
                    "Security Guard: Detected high-risk credential ({}: {}) in code diff. Commit aborted to protect your account.",
                    name, masked
                )),
            };
        }
    }

    SecurityAudit {
        is_safe: true,
        violation_reason: None,
    }
}

/// Redact sensitive secrets from diff summaries before sending them to external AI APIs
pub fn sanitize_diff_for_ai(diff_summary: &str) -> String {
    let mut sanitized = diff_summary.to_string();

    for (_name, regex) in SECRET_PATTERNS.iter() {
        sanitized = regex.replace_all(&sanitized, "[REDACTED_SECRET]").to_string();
    }

    sanitized
}

/// Sanitize and canonicalize a project path to prevent Path Traversal attacks
pub fn validate_and_canonicalize_path(input_path: &str) -> Result<String> {
    let clean = input_path.trim();
    if clean.is_empty() {
        bail!("Project path cannot be empty.");
    }

    // Reject dangerous directory traversal characters
    if clean.contains('\0') {
        bail!("Invalid null byte in path.");
    }

    let path = Path::new(clean);
    if !path.exists() {
        bail!("Path '{}' does not exist.", clean);
    }

    // Verify it is a directory
    if !path.is_dir() {
        bail!("Path '{}' is not a directory.", clean);
    }

    // Check system directory bounds
    let normalized = clean.replace('/', "\\").to_lowercase();
    let forbidden_roots = [
        "c:\\windows",
        "c:\\program files",
        "c:\\program files (x86)",
        "/usr",
        "/bin",
        "/etc",
        "/sbin",
    ];

    for root in forbidden_roots {
        if normalized.starts_with(root) {
            bail!("Access to system directory '{}' is forbidden.", clean);
        }
    }

    Ok(clean.to_string())
}

/// Sanitize commit prefix to prevent injection
pub fn sanitize_commit_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim();
    // Allow alphanumeric characters, emojis, dashes, colons, brackets, and spaces
    let sanitized: String = trimmed
        .chars()
        .filter(|&c| c.is_alphanumeric() || " :-_[]🔒✨⚡🚀🐛📝🛠️".contains(c))
        .take(50)
        .collect();

    if sanitized.is_empty() {
        "🔒 lid-close:".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_detection_aws() {
        let diff = "+ let key = \"AKIAIOSFODNN7EXAMPLE\";";
        let audit = audit_repository(&["src/main.rs"], diff);
        assert!(!audit.is_safe);
        assert!(audit.violation_reason.unwrap().contains("AWS Access Key"));
    }

    #[test]
    fn test_secret_detection_gemini() {
        let diff = "+ const apiKey = 'AIzaSyD-1234567890abcdefghijklmnopqrstuvw';";
        let audit = audit_repository(&["src/ai.rs"], diff);
        assert!(!audit.is_safe);
        assert!(audit.violation_reason.unwrap().contains("Google / Gemini API Key"));
    }

    #[test]
    fn test_sensitive_file_blocking() {
        let audit = audit_repository(&[".env.production", "src/app.js"], "");
        assert!(!audit.is_safe);
        assert!(audit.violation_reason.unwrap().contains(".env.production"));
    }

    #[test]
    fn test_dangerous_extension_blocking() {
        let audit = audit_repository(&["certs/server.pem"], "");
        assert!(!audit.is_safe);
        assert!(audit.violation_reason.unwrap().contains(".pem"));
    }

    #[test]
    fn test_safe_repository() {
        let diff = "+ fn calculate(a: i32, b: i32) -> i32 { a + b }";
        let audit = audit_repository(&["src/math.rs"], diff);
        assert!(audit.is_safe);
        assert!(audit.violation_reason.is_none());
    }

    #[test]
    fn test_diff_redaction_for_ai() {
        let diff = "API key was AIzaSyA1234567890abcdefghijklmnopqrstuvw here";
        let redacted = sanitize_diff_for_ai(diff);
        assert!(!redacted.contains("AIzaSy"));
        assert!(redacted.contains("[REDACTED_SECRET]"));
    }

    #[test]
    fn test_sanitize_commit_prefix() {
        let evil_prefix = "🔒 lid-close; rm -rf /";
        let clean = sanitize_commit_prefix(evil_prefix);
        assert!(!clean.contains(';'));
        assert!(!clean.contains('/'));
    }
}
