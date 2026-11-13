use crate::{
    db::Database,
    models::{NewTrack, Playlist, Track},
};
use anyhow::{Result, bail};
use rusqlite::{Row, params};

fn track_from_row(row: &Row<'_>) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get(0)?,
        title: row.get(1)?,
        artist: row.get(2)?,
        source_url: row.get(3)?,
        file_path: row.get(4)?,
        duration_ms: row.get(5)?,
        created_at: row.get(6)?,
    })
}

#[derive(Clone)]
pub struct TrackRepository {
    db: Database,
}

impl TrackRepository {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn insert(&self, track: &NewTrack) -> Result<Track> {
        let conn = self.db.connect()?;
        conn.execute("INSERT INTO tracks (title, artist, source_url, file_path, duration_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![track.title, track.artist, track.source_url, track.file_path, track.duration_ms])?;
        Ok(conn.query_row(
            "SELECT * FROM tracks WHERE id = ?1",
            [conn.last_insert_rowid()],
            track_from_row,
        )?)
    }

    pub fn list_tracks(&self) -> Result<Vec<Track>> {
        let conn = self.db.connect()?;
        let mut statement = conn.prepare("SELECT * FROM tracks ORDER BY id DESC")?;
        Ok(statement
            .query_map([], track_from_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn get(&self, id: i64) -> Result<Track> {
        Ok(self.db.connect()?.query_row(
            "SELECT * FROM tracks WHERE id = ?1",
            [id],
            track_from_row,
        )?)
    }

    pub fn delete(&self, id: i64) -> Result<bool> {
        Ok(self
            .db
            .connect()?
            .execute("DELETE FROM tracks WHERE id=?1", [id])?
            > 0)
    }
}

#[derive(Clone)]
pub struct PlaylistRepository {
    db: Database,
}

impl PlaylistRepository {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn create(&self, name: &str) -> Result<i64> {
        let conn = self.db.connect()?;
        conn.execute("INSERT INTO playlists (name) VALUES (?1)", [name])?;
        Ok(conn.last_insert_rowid())
    }

    pub fn delete(&self, id: i64) -> Result<Vec<String>> {
        let mut conn = self.db.connect()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let assets = {
            let mut query = tx.prepare("SELECT cover_path FROM playlist_appearance WHERE playlist_id=?1 AND cover_path IS NOT NULL UNION SELECT original_path FROM backgrounds WHERE playlist_id=?1 UNION SELECT display_path FROM backgrounds WHERE playlist_id=?1")?;
            query
                .query_map([id], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<String>>>()?
        };
        if tx.execute("DELETE FROM playlists WHERE id=?1", [id])? == 0 {
            bail!("Плейлист не найден");
        }
        tx.commit()?;
        Ok(assets)
    }

    pub fn list_playlists(&self) -> Result<Vec<Playlist>> {
        let conn = self.db.connect()?;
        let mut statement = conn.prepare("SELECT p.id, p.name, p.created_at, COUNT(pt.track_id), a.cover_path, a.cover_color, a.header_color, COALESCE(SUM(t.duration_ms), 0) FROM playlists p LEFT JOIN playlist_tracks pt ON pt.playlist_id = p.id LEFT JOIN tracks t ON t.id=pt.track_id LEFT JOIN playlist_appearance a ON a.playlist_id = p.id GROUP BY p.id ORDER BY p.id")?;
        Ok(statement
            .query_map([], |r| {
                Ok(Playlist {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                    track_count: r.get(3)?,
                    cover_path: r.get(4)?,
                    cover_color: r.get(5)?,
                    header_color: r.get(6)?,
                    duration_ms: r.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_cover(&self, id: i64, path: Option<&str>, color: Option<u32>) -> Result<()> {
        self.db.connect()?.execute(
            "INSERT INTO playlist_appearance (playlist_id, cover_path, cover_color) VALUES (?1, ?2, ?3) ON CONFLICT(playlist_id) DO UPDATE SET cover_path=excluded.cover_path, cover_color=excluded.cover_color",
            params![id, path, color],
        )?;
        Ok(())
    }

    pub fn set_header_color(&self, id: i64, color: Option<u32>) -> Result<()> {
        self.db.connect()?.execute(
            "INSERT INTO playlist_appearance (playlist_id, header_color) VALUES (?1, ?2) ON CONFLICT(playlist_id) DO UPDATE SET header_color=excluded.header_color",
            params![id, color],
        )?;
        Ok(())
    }

    /// Serializes concurrent appends so each track gets a unique final position.
    pub fn add_track(&self, playlist_id: i64, track_id: i64) -> Result<bool> {
        let mut conn = self.db.connect()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM playlists WHERE id=?1)",
            [playlist_id],
            |r| r.get(0),
        )?;
        if !exists {
            bail!("Плейлист не найден");
        }
        let duplicate: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM playlist_tracks WHERE playlist_id=?1 AND track_id=?2)",
            params![playlist_id, track_id],
            |r| r.get(0),
        )?;
        if duplicate {
            return Ok(false);
        }
        tx.execute("INSERT INTO playlist_tracks (playlist_id, track_id, position) SELECT ?1, ?2, COALESCE(MAX(position) + 1, 0) FROM playlist_tracks WHERE playlist_id=?1", params![playlist_id, track_id])?;
        tx.commit()?;
        Ok(true)
    }

    pub fn list_playlist_tracks(&self, playlist_id: i64) -> Result<Vec<Track>> {
        let conn = self.db.connect()?;
        let mut statement = conn.prepare("SELECT t.* FROM tracks t JOIN playlist_tracks pt ON pt.track_id=t.id WHERE pt.playlist_id=?1 ORDER BY pt.position")?;
        Ok(statement
            .query_map([playlist_id], track_from_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn remove_track(&self, playlist_id: i64, track_id: i64) -> Result<bool> {
        let mut conn = self.db.connect()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let removed = tx.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id=?1 AND track_id=?2",
            params![playlist_id, track_id],
        )? > 0;
        if removed {
            let ids = {
                let mut statement = tx.prepare(
                    "SELECT track_id FROM playlist_tracks WHERE playlist_id=?1 ORDER BY position",
                )?;
                statement
                    .query_map([playlist_id], |r| r.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            };
            let offset: i64 = tx.query_row(
                "SELECT COALESCE(MAX(position)+1,0) FROM playlist_tracks WHERE playlist_id=?1",
                [playlist_id],
                |r| r.get(0),
            )?;
            // Move all positions out of the target range before compacting (UNIQUE is immediate).
            tx.execute(
                "UPDATE playlist_tracks SET position=position+?2 WHERE playlist_id=?1",
                params![playlist_id, offset],
            )?;
            for (position, id) in ids.into_iter().enumerate() {
                tx.execute(
                    "UPDATE playlist_tracks SET position=?3 WHERE playlist_id=?1 AND track_id=?2",
                    params![playlist_id, id, position as i64],
                )?;
            }
        }
        tx.commit()?;
        Ok(removed)
    }
}
