//! Anthropic API client (cloud opt-in path).
//!
//! Token storage: `~/.config/furugura/anthropic_token` mode 0600.
//! Token is read at runtime, used as `x-api-key` header — never logged.
//!
//! The consent gate (interactive prompt + attendee guard) is decided in
//! `summarize::consent`, *not* here. By the time `messages` is called,
//! the caller has already obtained consent.

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

pub const ANTHROPIC_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
pub const ANTHROPIC_API_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone)]
pub struct AnthropicRequest {
    pub api_token: String,
    pub model: String,
    pub system: String,
    pub user: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize)]
struct MessagesBody<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<MessageIn<'a>>,
}

#[derive(Debug, Clone, Serialize)]
struct MessageIn<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessagesResponse {
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    #[serde(other)]
    Other,
}

/// Read the token from disk. Errors if the file is missing or wider than
/// mode 0600 — the latter prevents a cloud call with a token someone
/// else can read.
pub fn load_token(path: &Path) -> Result<String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).with_context(|| {
        format!(
            "anthropic token not found at {} — run `furu setup` or store one with mode 0600",
            path.display(),
        )
    })?;
    let mode = meta.permissions().mode() & 0o077;
    if mode != 0 {
        return Err(anyhow!(
            "anthropic token at {} has loose permissions ({:o}); \
             run `chmod 0600 {}`",
            path.display(),
            meta.permissions().mode() & 0o777,
            path.display(),
        ));
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return Err(anyhow!("anthropic token file is empty: {}", path.display()));
    }
    Ok(trimmed)
}

pub async fn messages(req: AnthropicRequest) -> Result<String> {
    let body = MessagesBody {
        model: &req.model,
        max_tokens: req.max_tokens,
        system: &req.system,
        messages: vec![MessageIn {
            role: "user",
            content: &req.user,
        }],
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()?;

    let resp = client
        .post(ANTHROPIC_ENDPOINT)
        .header("x-api-key", req.api_token)
        .header("anthropic-version", ANTHROPIC_API_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .context("could not reach Anthropic API")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Anthropic API returned HTTP {status}: {body_text}"));
    }

    let parsed: MessagesResponse = resp.json().await
        .context("Anthropic response was not JSON")?;
    let text = parsed
        .content
        .into_iter()
        .filter_map(|c| match c {
            ContentBlock::Text { text } => Some(text),
            ContentBlock::Other => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        return Err(anyhow!("Anthropic response had no text content"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn write_token(path: &std::path::Path, contents: &str, mode: u32) {
        std::fs::write(path, contents).unwrap();
        let perms = std::fs::Permissions::from_mode(mode);
        std::fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    fn load_token_happy_path() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("tok");
        write_token(&p, "sk-ant-xxxxxxxxxxxxxxxx\n", 0o600);
        let t = load_token(&p).unwrap();
        assert_eq!(t, "sk-ant-xxxxxxxxxxxxxxxx");
    }

    #[test]
    fn load_token_rejects_loose_permissions() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("tok");
        write_token(&p, "sk-ant-xxxxxxxxxxxxxxxx", 0o644);
        let err = load_token(&p).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("loose permissions"));
    }

    #[test]
    fn load_token_rejects_empty_file() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("tok");
        write_token(&p, "  \n", 0o600);
        let err = load_token(&p).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("empty"));
    }

    #[test]
    fn load_token_missing_file_has_helpful_error() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("nope");
        let err = load_token(&p).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("not found"));
    }
}
