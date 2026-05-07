//! Disk-backed token persistence at `~/.jarvis/auth/<provider>.json`.
//!
//! Files are written with mode 0o600 (owner read/write only) so a
//! shared machine can't trivially leak access tokens. Use
//! `JARVIS_AUTH_DIR` to point at an alternate directory in tests.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::tokens::Tokens;

#[derive(Debug, thiserror::Error)]
pub enum TokenStoreError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("parse: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid provider name: {0}")]
    InvalidProviderName(String),
}

pub struct TokenStore {
    dir: PathBuf,
}

impl TokenStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Default location: `$JARVIS_AUTH_DIR` or `~/.jarvis/auth/`.
    pub fn default_dir() -> Self {
        if let Ok(p) = std::env::var("JARVIS_AUTH_DIR") {
            return Self::new(PathBuf::from(p));
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Self::new(PathBuf::from(home).join(".jarvis").join("auth"))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path_for(&self, provider: &str) -> Result<PathBuf, TokenStoreError> {
        validate_provider_name(provider)?;
        Ok(self.dir.join(format!("{provider}.json")))
    }

    pub fn load(&self, provider: &str) -> Result<Option<Tokens>, TokenStoreError> {
        let path = self.path_for(provider)?;
        match fs::read_to_string(&path) {
            Ok(s) => Ok(Some(serde_json::from_str(&s)?)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, tokens: &Tokens) -> Result<PathBuf, TokenStoreError> {
        let path = self.path_for(&tokens.provider)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_string_pretty(tokens)?;
        fs::write(&path, body)?;
        set_owner_only(&path)?;
        Ok(path)
    }

    pub fn delete(&self, provider: &str) -> Result<bool, TokenStoreError> {
        let path = self.path_for(provider)?;
        match fs::remove_file(&path) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Names of all stored providers — useful for `model list`.
    pub fn list_providers(&self) -> Result<Vec<String>, TokenStoreError> {
        let mut out = Vec::new();
        let entries = match fs::read_dir(&self.dir) {
            Ok(it) => it,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        for ent in entries {
            let ent = ent?;
            let name = ent.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".json") {
                out.push(stem.to_string());
            }
        }
        out.sort();
        Ok(out)
    }
}

fn validate_provider_name(name: &str) -> Result<(), TokenStoreError> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.starts_with('.')
    {
        return Err(TokenStoreError::InvalidProviderName(name.to_string()));
    }
    Ok(())
}

#[cfg(unix)]
fn set_owner_only(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> io::Result<()> {
    Ok(())
}
