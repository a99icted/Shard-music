use anyhow::{Context, Result, bail};
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Database {
    root: PathBuf,
}

impl Database {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(root.join("audio"))?;
        std::fs::create_dir_all(root.join("staging"))?;
        let db = Self { root };
        let mut conn = db.connect()?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > 4 {
            bail!("База данных создана более новой версией Shard");
        }
        let tx = conn.transaction()?;
        tx.execute_batch(include_str!("schema.sql"))?;
        tx.pragma_update(None, "user_version", 4)?;
        tx.commit()?;
        Ok(db)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(self.root.join("library.sqlite3"))
            .context("Не удалось открыть SQLite")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        // SQLite requires this on EVERY connection, not just at initialization.
        conn.pragma_update(None, "foreign_keys", true)?;
        Ok(conn)
    }
}
