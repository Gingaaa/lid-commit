use anyhow::{Context, Result};
use tracing::{debug, info};

use crate::config::Config;

/// Status of a git repository's working tree
#[derive(Debug)]
pub struct RepoStatus {
    /// True if there are uncommitted changes
    pub has_changes: bool,
    /// True if the repo has a remote configured
    pub has_remote: bool,
    /// Number of changed/untracked files
    pub changed_files: usize,
    /// True if there are merge conflicts
    pub has_conflicts: bool,
    /// Summarized diff for AI message generation (file list + stats)
    pub diff_summary: String,
    /// Current branch name
    pub branch: String,
}

impl RepoStatus {
    /// Check safety guards. Returns `Some(reason)` if we should skip committing.
    pub fn should_skip(&self, config: &Config) -> Option<String> {
        if !self.has_remote && config.safety.require_remote {
            return Some("no remote configured".into());
        }

        if self.has_conflicts && config.safety.skip_on_conflicts {
            return Some("merge conflicts present".into());
        }

        if self.changed_files > config.safety.max_files_changed {
            return Some(format!(
                "too many changed files ({} > {})",
                self.changed_files, config.safety.max_files_changed
            ));
        }

        None
    }
}

/// Check the git status of a project directory
pub async fn check_project(path: &str, config: &Config) -> Result<RepoStatus> {
    // 1. Get current branch
    let branch = run_git(path, &["rev-parse", "--abbrev-ref", "HEAD"])
        .await
        .unwrap_or_else(|_| "unknown".to_string());
    let branch = branch.trim().to_string();

    // 2. Check for remote
    let remote_output = run_git(path, &["remote"]).await.unwrap_or_default();
    let has_remote = !remote_output.trim().is_empty();

    // 3. Check for changes (porcelain format: each changed file is one line)
    let status_output = run_git(path, &["status", "--porcelain"]).await?;
    let changed_lines: Vec<&str> = status_output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    let changed_files = changed_lines.len();
    let has_changes = changed_files > 0;

    // 4. Check for merge conflicts (lines starting with UU, AA, DD)
    let has_conflicts = changed_lines.iter().any(|l| {
        l.starts_with("UU") || l.starts_with("AA") || l.starts_with("DD")
    });

    // 5. Build diff summary for AI
    let diff_summary = if has_changes {
        build_diff_summary(path, config.ai.max_diff_lines).await
    } else {
        String::new()
    };

    debug!(
        "Git status for {}: {} changed files, branch={}, remote={}",
        path, changed_files, branch, has_remote
    );

    Ok(RepoStatus {
        has_changes,
        has_remote,
        changed_files,
        has_conflicts,
        diff_summary,
        branch,
    })
}

/// Build a diff summary to send to the AI for commit message generation.
/// Includes: changed file list + abbreviated diff (first N lines)
async fn build_diff_summary(path: &str, max_lines: usize) -> String {
    let mut parts = Vec::new();

    // File list from `git diff --name-status` (staged + unstaged)
    if let Ok(name_status) = run_git(path, &["diff", "--name-status", "HEAD"]).await {
        if !name_status.trim().is_empty() {
            parts.push(format!("Changed files:\n{}", name_status));
        }
    }

    // Also include untracked files
    if let Ok(untracked) = run_git(
        path,
        &["ls-files", "--others", "--exclude-standard"],
    )
    .await
    {
        if !untracked.trim().is_empty() {
            parts.push(format!("New (untracked) files:\n{}", untracked));
        }
    }

    // Abbreviated diff for context (up to max_lines)
    if let Ok(diff) = run_git(path, &["diff", "--unified=3", "HEAD"]).await {
        let lines: Vec<&str> = diff.lines().take(max_lines).collect();
        if !lines.is_empty() {
            parts.push(format!("Diff (truncated to {} lines):\n{}", max_lines, lines.join("\n")));
        }
    }

    parts.join("\n\n")
}

/// Stage all changes, create a commit, and optionally push
pub async fn commit_and_push(path: &str, message: &str, push: bool) -> Result<bool> {
    info!("  📝 Staging all changes in {}...", path);

    // Stage everything (respects .gitignore)
    run_git(path, &["add", "--all"])
        .await
        .context("git add --all failed")?;

    info!("  💾 Committing with message: {}", message);

    // Commit
    run_git(path, &["commit", "-m", message])
        .await
        .context("git commit failed")?;

    // Optionally push
    if push {
        info!("  🚀 Pushing to remote...");
        match run_git(path, &["push"]).await {
            Ok(_) => {
                info!("  ✅ Push successful.");
                return Ok(true);
            }
            Err(e) => {
                // Push failed (e.g., no upstream set) — commit is still saved locally
                tracing::warn!("  ⚠️  Push failed (commit saved locally): {}", e);
                return Ok(false);
            }
        }
    }

    Ok(false)
}

/// Run a git command in the given directory and return its stdout
async fn run_git(path: &str, args: &[&str]) -> Result<String> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .await
        .with_context(|| format!("Failed to run git {:?} in {}", args, path))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "git {:?} failed in {}: {}",
            args,
            path,
            stderr.trim()
        )
    }
}
