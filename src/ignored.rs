//! Local playback exclusions keep library and user playlist membership.
use crate::db::Database;
use anyhow::Result;
use std::collections::HashSet;

#[derive(Clone)]
pub struct IgnoredTracksService {
    db: Database,
}

impl IgnoredTracksService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn ids(&self) -> Result<HashSet<i64>> {
        let conn = self.db.connect()?;
        Ok(conn
            .prepare("SELECT track_id FROM ignored_tracks")?
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn set(&self, id: i64, ignored: bool) -> Result<bool> {
        let mut conn = self.db.connect()?;
        let tx = conn.transaction()?;
        let changed = if ignored {
            let changed = tx.execute(
                "INSERT OR IGNORE INTO ignored_tracks(track_id) VALUES (?1)",
                [id],
            )?;
            tx.execute("DELETE FROM favorites WHERE track_id=?1", [id])?;
            changed
        } else {
            tx.execute("DELETE FROM ignored_tracks WHERE track_id=?1", [id])?
        };
        tx.commit()?;
        Ok(changed > 0)
    }
}
