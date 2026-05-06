use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("immutable log violation: {0}")]
    Immutable(String),

    #[error("constraint: {0}")]
    Constraint(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type DbResult<T> = std::result::Result<T, DbError>;
