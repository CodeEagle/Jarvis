//! On-disk loader / saver for `LlmConfig`.
//!
//! The default config lives at `~/.jarvis/config.toml` and is parsed
//! as TOML. Override with the `JARVIS_CONFIG` env var (used by tests
//! and unusual setups).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::LlmConfig;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("parse: {0}")]
    Parse(String),
    #[error("serialize: {0}")]
    Serialize(String),
}

/// Resolve the config path:
///   1. `$JARVIS_CONFIG` if set
///   2. `~/.jarvis/config.toml` otherwise
pub fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("JARVIS_CONFIG") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".jarvis").join("config.toml")
}

pub fn load_from_path(path: &Path) -> Result<LlmConfig, ConfigError> {
    let s = fs::read_to_string(path)?;
    toml::from_str(&s).map_err(|e| ConfigError::Parse(e.to_string()))
}

/// Load the default config; returns `LlmConfig::default()` when the
/// file does not exist (first-run case).
pub fn load_default() -> Result<LlmConfig, ConfigError> {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(s) => toml::from_str(&s).map_err(|e| ConfigError::Parse(e.to_string())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(LlmConfig::default()),
        Err(e) => Err(ConfigError::Io(e)),
    }
}

pub fn save_to_path(path: &Path, cfg: &LlmConfig) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let s = toml::to_string_pretty(cfg).map_err(|e| ConfigError::Serialize(e.to_string()))?;
    fs::write(path, s)?;
    Ok(())
}

pub fn save_default(cfg: &LlmConfig) -> Result<(), ConfigError> {
    save_to_path(&config_path(), cfg)
}

/// Env var name that holds the API key for `provider`. Falls back to
/// well-known per-provider names when the config is silent.
pub fn provider_env_var(provider: &str, cfg: &LlmConfig) -> Option<String> {
    if let Some(p) = cfg.providers.get(provider) {
        if let Some(env) = &p.api_key_env {
            return Some(env.clone());
        }
    }
    let well_known = match provider {
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "gemini" => "GEMINI_API_KEY",
        "groq" => "GROQ_API_KEY",
        "cohere" => "COHERE_API_KEY",
        "deepseek" => "DEEPSEEK_API_KEY",
        "xai" => "XAI_API_KEY",
        "fireworks" => "FIREWORKS_API_KEY",
        "together" => "TOGETHER_API_KEY",
        // Local / self-hosted: no auth needed.
        "ollama" | "ollama_cloud" => return None,
        _ => return None,
    };
    Some(well_known.to_string())
}

/// CLI binary that holds the OAuth credentials for an OAuth-only
/// provider. When present, [`provider_authed_full`] short-circuits the
/// env-var check and returns "ready" iff the binary is reachable on
/// `$PATH` (the provider's own CLI handles token storage and refresh).
pub fn provider_oauth_binary(provider: &str) -> Option<&'static str> {
    match provider {
        // Subprocess wrapper around Anthropic's `claude` CLI. Auth is
        // OAuth held in `~/.claude/`; we just need the binary.
        "claude-cli" => Some("claude"),
        _ => None,
    }
}

/// Auth check with both an env getter and a binary-presence checker
/// injected — needed for OAuth providers where readiness depends on
/// a sibling CLI being installed rather than an env var being set.
pub fn provider_authed_full<E, B>(
    provider: &str,
    cfg: &LlmConfig,
    env_get: E,
    binary_present: B,
) -> bool
where
    E: Fn(&str) -> Option<String>,
    B: Fn(&str) -> bool,
{
    if let Some(bin) = provider_oauth_binary(provider) {
        return binary_present(bin);
    }
    let Some(env) = provider_env_var(provider, cfg) else {
        // Local provider — no auth required.
        return matches!(provider, "ollama" | "ollama_cloud");
    };
    env_get(&env).map(|v| !v.is_empty()).unwrap_or(false)
}

/// Backwards-compatible wrapper that assumes no OAuth binaries are
/// present. Existing tests call this with just an env getter.
pub fn provider_authed_with<F>(provider: &str, cfg: &LlmConfig, env_get: F) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    provider_authed_full(provider, cfg, env_get, |_| false)
}

/// Process-env convenience over [`provider_authed_full`] — checks both
/// the well-known env var and `$PATH` for OAuth providers.
pub fn provider_authed(provider: &str, cfg: &LlmConfig) -> bool {
    provider_authed_full(
        provider,
        cfg,
        |k| std::env::var(k).ok(),
        binary_on_path,
    )
}

/// True iff `name` resolves to an executable file on `$PATH`. Pure
/// stdlib so we don't pull a `which`-style dep.
pub fn binary_on_path(name: &str) -> bool {
    let path_env = match std::env::var_os("PATH") {
        Some(p) => p,
        None => return false,
    };
    for dir in std::env::split_paths(&path_env) {
        let candidate = dir.join(name);
        if !candidate.is_file() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = candidate.metadata() {
                if meta.permissions().mode() & 0o111 != 0 {
                    return true;
                }
            }
        }
        #[cfg(not(unix))]
        {
            return true;
        }
    }
    false
}
