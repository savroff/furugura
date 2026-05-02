use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::paths::Paths;

/// User-facing configuration. Loaded from `~/.config/furugura/config.toml`,
/// with each field overridable via `FURU_*` environment variables.
///
/// Missing fields use their defaults. Unknown fields are tolerated to keep
/// forward-compat (`furugura_version: 1` discriminator lives in the markdown
/// frontmatter, not here).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// GGUF model used for the batch (finalize) whisper.cpp pass.
    pub whisper_model_batch: String,

    /// GGUF model used for the streaming whisper.cpp pass.
    pub whisper_model_stream: String,

    /// Ollama model name (or `claude-...` style for cloud).
    pub summary_model: String,

    /// `local` (Ollama) or `cloud:anthropic`.
    pub summary_provider: SummaryProvider,

    /// Ollama HTTP endpoint. Honors `OLLAMA_HOST` env var at runtime when unset.
    pub summary_endpoint: Option<String>,

    /// Whisper / Ollama context window for the summary prompt assembly.
    pub num_ctx: u32,

    /// Where finalized meeting markdown is written. Default: `~/Meetings/`.
    pub output_dir: Option<PathBuf>,

    /// Where the HuggingFace token lives. Default: `~/.config/furugura/hf_token`.
    pub hf_token_path: Option<PathBuf>,

    /// PCM sample rate for both pw-record subprocesses.
    pub audio_rate: u32,

    /// Default for `--keep-audio`. The CLI flag still wins per-meeting.
    pub keep_audio: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryProvider {
    Local,
    #[serde(rename = "cloud:anthropic")]
    CloudAnthropic,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            whisper_model_batch: "large-v3".to_string(),
            whisper_model_stream: "base.en".to_string(),
            summary_model: "gemma3:4b".to_string(),
            summary_provider: SummaryProvider::Local,
            summary_endpoint: None,
            num_ctx: 16384,
            output_dir: None,
            hf_token_path: None,
            audio_rate: 48_000,
            keep_audio: false,
        }
    }
}

impl Config {
    /// Load from `path` if given, otherwise from `~/.config/furugura/config.toml`.
    /// Missing file is not an error — defaults are returned.
    /// `FURU_*` env vars (e.g. `FURU_SUMMARY_MODEL`) override after parse.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let paths = Paths::discover()?;
        let path = match path {
            Some(p) => p.to_path_buf(),
            None => paths.config_file(),
        };

        let mut cfg = if path.exists() {
            Self::load_from_file(&path)?
        } else {
            Self::default()
        };
        cfg.apply_env_overrides();
        Ok(cfg)
    }

    /// Parse a config file. Public for tests; `load` is the normal entry point.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("could not read config: {}", path.display()))?;
        toml::from_str(&raw)
            .with_context(|| format!("invalid config TOML: {}", path.display()))
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("FURU_WHISPER_MODEL_BATCH") {
            self.whisper_model_batch = v;
        }
        if let Ok(v) = std::env::var("FURU_WHISPER_MODEL_STREAM") {
            self.whisper_model_stream = v;
        }
        if let Ok(v) = std::env::var("FURU_SUMMARY_MODEL") {
            self.summary_model = v;
        }
        if let Ok(v) = std::env::var("FURU_SUMMARY_ENDPOINT") {
            self.summary_endpoint = Some(v);
        }
        if let Ok(v) = std::env::var("FURU_OUTPUT_DIR") {
            self.output_dir = Some(PathBuf::from(v));
        }
        if let Ok(v) = std::env::var("FURU_AUDIO_RATE")
            && let Ok(n) = v.parse::<u32>()
        {
            self.audio_rate = n;
        }
        if let Ok(v) = std::env::var("FURU_KEEP_AUDIO") {
            self.keep_audio = matches!(v.as_str(), "1" | "true" | "yes" | "on");
        }
    }

    pub fn output_dir_or(&self, paths: &Paths) -> PathBuf {
        self.output_dir
            .clone()
            .unwrap_or_else(|| paths.default_output_dir.clone())
    }

    pub fn hf_token_path_or(&self, paths: &Paths) -> PathBuf {
        self.hf_token_path
            .clone()
            .unwrap_or_else(|| paths.hf_token_file())
    }
}
