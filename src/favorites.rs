//! Favorites are global track memberships, independent of user playlists.
use crate::{
    db::Database,
    i18n::tr,
    models::{Playlist, Track},
    repositories::TrackRepository,
};
use anyhow::Result;
use std::collections::HashSet;

pub const FAVORITES_ID: i64 = -1;

#[derive(Clone)]
pub struct FavoritesService {
    db: Database,
}
impl FavoritesService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    pub fn ids(&self) -> Result<HashSet<i64>> {
        let conn = self.db.connect()?;
        Ok(conn
            .prepare("SELECT track_id FROM favorites")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn set(&self, id: i64, favorite: bool) -> Result<bool> {
        let conn = self.db.connect()?;
        Ok(if favorite {
            conn.execute(
                "INSERT OR IGNORE INTO favorites(track_id) VALUES (?1)",
                [id],
            )?
        } else {
            conn.execute("DELETE FROM favorites WHERE track_id=?1", [id])?
        } > 0)
    }
    pub fn tracks(&self) -> Result<Vec<Track>> {
        let conn = self.db.connect()?;
        let ids = conn
            .prepare("SELECT track_id FROM favorites ORDER BY added_at DESC, rowid DESC")?
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let repo = TrackRepository::new(self.db.clone());
        ids.into_iter().map(|id| repo.get(id)).collect()
    }
    pub fn playlist(&self) -> Result<Playlist> {
        let conn = self.db.connect()?;
        let (count, duration) = conn.query_row("SELECT COUNT(*), COALESCE(SUM(t.duration_ms),0) FROM favorites f JOIN tracks t ON t.id=f.track_id", [], |r| Ok((r.get(0)?,r.get(1)?)))?;
        Ok(Playlist {
            id: FAVORITES_ID,
            name: tr("Избранное").into(),
            created_at: String::new(),
            track_count: count,
            duration_ms: duration,
            cover_path: None,
            cover_color: None,
            header_color: None,
        })
    }
}
