//! Ollama HTTP client for the local-by-default summarization path.
//!
//! Endpoint resolution: `OLLAMA_HOST` env > config `summary_endpoint` >
//! default `http://localhost:11434`. If the resolved host is not loopback,
//! we emit a one-line warning before invoking — local-by-default is
//! enforced at runtime, not assumed.

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

pub const DEFAULT_OLLAMA_ENDPOINT: &str = "http://localhost:11434";

#[derive(Debug, Clone)]
pub struct OllamaRequest {
    pub endpoint: String,
    pub model: String,
    pub prompt: String,
    pub num_ctx: u32,
    pub temperature: f32,
    pub keep_alive: String,
}

#[derive(Debug, Clone, Serialize)]
struct GenerateBody<'a> {
    model: &'a str,
    prompt: &'a str,
    options: GenerateOptions,
    keep_alive: &'a str,
    stream: bool,
}

#[derive(Debug, Clone, Serialize)]
struct GenerateOptions {
    num_ctx: u32,
    temperature: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenerateResponse {
    pub response: String,
    #[serde(default)]
    pub done: bool,
}

/// Resolve the Ollama endpoint, with `env` (typically `OLLAMA_HOST`)
/// taking precedence over the optional config value, and falling back
/// to localhost.
pub fn resolve_endpoint(env: Option<&str>, config: Option<&str>) -> String {
    env.map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .or_else(|| config.map(|s| s.trim()).filter(|s| !s.is_empty()))
        .map(normalize_endpoint)
        .unwrap_or_else(|| DEFAULT_OLLAMA_ENDPOINT.to_string())
}

/// Some users set `OLLAMA_HOST=foo:11434` (no scheme); reqwest needs one.
/// Add `http://` if the input parses as a bare host:port or hostname.
fn normalize_endpoint(s: &str) -> String {
    if s.starts_with("http://") || s.starts_with("https://") {
        s.to_string()
    } else {
        format!("http://{s}")
    }
}

/// Whether the resolved endpoint's host is loopback.
/// Returns `false` (i.e. "warn the user") on parse failure so we err on
/// the side of disclosure.
pub fn endpoint_is_loopback(endpoint: &str) -> bool {
    let url = match Url::parse(endpoint) {
        Ok(u) => u,
        Err(_) => return false,
    };
    match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}

/// Perform the HTTP call. Caller is responsible for printing the
/// loopback warning before invoking.
pub async fn generate(req: OllamaRequest) -> Result<String> {
    let url = format!("{}/api/generate", req.endpoint.trim_end_matches('/'));
    let body = GenerateBody {
        model: &req.model,
        prompt: &req.prompt,
        options: GenerateOptions {
            num_ctx: req.num_ctx,
            temperature: req.temperature,
        },
        keep_alive: &req.keep_alive,
        stream: false,
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()?;

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("could not reach Ollama at {}", req.endpoint))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Ollama returned HTTP {status}: {body_text}\n\
             hint: run `ollama serve` or check `furu setup`",
        ));
    }

    let parsed: GenerateResponse = resp.json().await
        .context("Ollama response was not JSON")?;
    Ok(parsed.response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_endpoint_default() {
        assert_eq!(resolve_endpoint(None, None), DEFAULT_OLLAMA_ENDPOINT);
    }

    #[test]
    fn resolve_endpoint_env_wins_over_config() {
        let r = resolve_endpoint(
            Some("http://gpu-box:11434"),
            Some("http://other-box:11434"),
        );
        assert_eq!(r, "http://gpu-box:11434");
    }

    #[test]
    fn resolve_endpoint_blank_env_falls_through_to_config() {
        let r = resolve_endpoint(Some(""), Some("http://other-box:11434"));
        assert_eq!(r, "http://other-box:11434");
    }

    #[test]
    fn resolve_endpoint_normalizes_missing_scheme() {
        let r = resolve_endpoint(Some("gpu-box:11434"), None);
        assert_eq!(r, "http://gpu-box:11434");
    }

    #[test]
    fn loopback_default_is_loopback() {
        assert!(endpoint_is_loopback(DEFAULT_OLLAMA_ENDPOINT));
        assert!(endpoint_is_loopback("http://127.0.0.1:11434"));
        assert!(endpoint_is_loopback("http://[::1]:11434"));
        assert!(endpoint_is_loopback("http://Localhost:11434"));
    }

    #[test]
    fn non_loopback_is_not_loopback() {
        assert!(!endpoint_is_loopback("http://gpu-box:11434"));
        assert!(!endpoint_is_loopback("http://10.0.0.5:11434"));
        assert!(!endpoint_is_loopback("https://server.example.com:11434"));
    }

    #[test]
    fn invalid_url_treated_as_non_loopback() {
        assert!(!endpoint_is_loopback("not a url"));
        assert!(!endpoint_is_loopback(""));
    }
}
