use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::error::DbResult;
use crate::migrations;

/// A handle to the Jarvis SQLite database.
///
/// `rusqlite::Connection` is not `Sync`; we wrap it in a `Mutex` so the
/// handle can be cloned across tasks. SQLite serializes write transactions
/// internally, so the mutex isn't a meaningful additional bottleneck for
/// our workload.
#[derive(Clone)]
pub struct Db {
    inner: Arc<Mutex<Connection>>,
}

impl Db {
    /// Open the database at `path`, creating it (and the schema) if needed.
    pub fn open(path: impl AsRef<Path>) -> DbResult<Self> {
        let conn = Connection::open(path)?;
        Self::with_connection(conn)
    }

    /// Open an in-memory database. Useful for tests.
    pub fn in_memory() -> DbResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::with_connection(conn)
    }

    fn with_connection(conn: Connection) -> DbResult<Self> {
        // Sensible defaults. WAL improves concurrent reads, foreign_keys
        // enforces referential integrity. recursive_triggers ensures our
        // immutability triggers fire even from cascaded operations.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA recursive_triggers = ON;",
        )?;
        migrations::run(&conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    /// Run a closure with a locked connection. Most callers should prefer
    /// the typed repo APIs in `memory_repo` / `session_repo` / etc.
    pub fn with<R>(&self, f: impl FnOnce(&Connection) -> DbResult<R>) -> DbResult<R> {
        let lock = self.inner.lock().expect("db mutex poisoned");
        f(&lock)
    }
}
