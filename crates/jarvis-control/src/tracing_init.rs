//! Structured tracing initialiser.
//!
//! Pulls the env filter from `JARVIS_LOG` (or `RUST_LOG` as a fallback)
//! and installs a `tracing-subscriber` layer that emits human-readable
//! lines on stdout. Callers swap in JSON output by setting
//! `JARVIS_LOG_JSON=1`.

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Idempotent: if a global subscriber is already installed, this is a
/// no-op. Returns `true` when it actually installed one.
pub fn init() -> bool {
    let filter = EnvFilter::try_from_env("JARVIS_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let json = std::env::var("JARVIS_LOG_JSON").ok().as_deref() == Some("1");
    let registry = tracing_subscriber::registry().with(filter);
    if json {
        registry
            .with(fmt::layer().with_target(true).json())
            .try_init()
            .is_ok()
    } else {
        registry
            .with(fmt::layer().with_target(true).compact())
            .try_init()
            .is_ok()
    }
}
