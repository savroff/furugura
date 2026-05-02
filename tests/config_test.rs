use furugura::config::{Config, SummaryProvider};
use std::path::Path;

fn write_tmp(name: &str, content: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(name), content).unwrap();
    dir
}

#[test]
fn happy_path_full_toml_deserializes() {
    let toml = r#"
        whisper_model_batch = "large-v3"
        whisper_model_stream = "base.en"
        summary_model = "gemma3:4b"
        summary_provider = "local"
        num_ctx = 16384
        audio_rate = 48000
        keep_audio = false
    "#;
    let dir = write_tmp("config.toml", toml);
    let cfg = Config::load_from_file(&dir.path().join("config.toml")).unwrap();
    assert_eq!(cfg.whisper_model_batch, "large-v3");
    assert_eq!(cfg.summary_model, "gemma3:4b");
    assert_eq!(cfg.summary_provider, SummaryProvider::Local);
    assert_eq!(cfg.num_ctx, 16384);
    assert_eq!(cfg.audio_rate, 48_000);
    assert!(!cfg.keep_audio);
}

#[test]
fn empty_toml_uses_defaults() {
    let dir = write_tmp("config.toml", "");
    let cfg = Config::load_from_file(&dir.path().join("config.toml")).unwrap();
    let defaults = Config::default();
    assert_eq!(cfg.whisper_model_batch, defaults.whisper_model_batch);
    assert_eq!(cfg.summary_model, defaults.summary_model);
    assert_eq!(cfg.audio_rate, defaults.audio_rate);
}

#[test]
fn partial_toml_fills_missing_fields_with_defaults() {
    let toml = r#"summary_model = "qwen3:1.7b""#;
    let dir = write_tmp("config.toml", toml);
    let cfg = Config::load_from_file(&dir.path().join("config.toml")).unwrap();
    assert_eq!(cfg.summary_model, "qwen3:1.7b");
    assert_eq!(cfg.whisper_model_batch, Config::default().whisper_model_batch);
    assert_eq!(cfg.audio_rate, Config::default().audio_rate);
}

#[test]
fn unknown_fields_are_tolerated() {
    // Forward-compat: a v1.5 config field must not fail v1 parse.
    let toml = r#"
        summary_model = "gemma3:4b"
        future_field_v15 = "ignored"
    "#;
    let dir = write_tmp("config.toml", toml);
    let cfg = Config::load_from_file(&dir.path().join("config.toml"));
    assert!(cfg.is_ok(), "unknown fields should be tolerated: {cfg:?}");
}

#[test]
fn invalid_toml_returns_error_with_path() {
    let dir = write_tmp("config.toml", "summary_model = ===bad===");
    let path = dir.path().join("config.toml");
    let err = Config::load_from_file(&path).unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("config.toml") || msg.contains(path.to_string_lossy().as_ref()),
        "error should mention the file path: {msg}",
    );
}

#[test]
fn missing_file_via_load_returns_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let nonexistent = dir.path().join("does-not-exist.toml");
    let cfg = Config::load(Some(&nonexistent)).unwrap();
    assert_eq!(cfg.audio_rate, Config::default().audio_rate);
}

#[test]
fn cloud_anthropic_provider_round_trips() {
    let toml = r#"
        summary_provider = "cloud:anthropic"
        summary_model = "claude-opus-4-7"
    "#;
    let dir = write_tmp("config.toml", toml);
    let cfg = Config::load_from_file(&dir.path().join("config.toml")).unwrap();
    assert_eq!(cfg.summary_provider, SummaryProvider::CloudAnthropic);
    assert_eq!(cfg.summary_model, "claude-opus-4-7");
}

// Sanity: paths module entry point compiles and produces non-empty paths.
#[test]
fn paths_discover_returns_populated_struct() {
    // Ensure HOME and XDG_RUNTIME_DIR are set in the test environment.
    if std::env::var_os("HOME").is_none() || std::env::var_os("XDG_RUNTIME_DIR").is_none() {
        eprintln!("skipping: HOME or XDG_RUNTIME_DIR not set in test environment");
        return;
    }
    let p = furugura::paths::Paths::discover().unwrap();
    assert!(!p.config_dir.as_os_str().is_empty());
    assert!(!p.runtime_dir.as_os_str().is_empty());
    assert!(!p.default_output_dir.as_os_str().is_empty());
    assert!(p.config_file().ends_with("config.toml"));
    assert_eq!(p.config_file().parent(), Some(p.config_dir.as_path() as &Path));
}
