use crate::db::Database;
use anyhow::Result;
use rusqlite::params;

#[derive(Clone)]
pub struct StatisticsService {
    db: Database,
}

#[derive(Clone, Default)]
pub struct ListeningSummary {
    pub month_ms: i64,
    pub year_ms: i64,
    pub top: Vec<TopTrack>,
}

#[derive(Clone)]
pub struct TopTrack {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub plays: i64,
}

impl StatisticsService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Persist measured audio time, never the seek position or the track duration.
    /// Calendar buckets use the listener's local date at the time of playback.
    pub fn record(&self, session: &str, track: i64, listened_ms: u64, at: i64) -> Result<()> {
        if listened_ms == 0 {
            return Ok(());
        }
        let mut conn = self.db.connect()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO listening_sessions(id,track_id,listened_ms) VALUES (?1,(SELECT id FROM tracks WHERE id=?2),?3)
             ON CONFLICT(id) DO UPDATE SET listened_ms=listening_sessions.listened_ms+excluded.listened_ms",
            params![session, track, listened_ms.min(i64::MAX as u64) as i64],
        )?;
        tx.execute(
            "INSERT INTO listening_days(day,listened_ms) VALUES (date(?1,'unixepoch','localtime'),?2)
             ON CONFLICT(day) DO UPDATE SET listened_ms=listening_days.listened_ms+excluded.listened_ms",
            params![at, listened_ms.min(i64::MAX as u64) as i64],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn summary(&self) -> Result<ListeningSummary> {
        let conn = self.db.connect()?;
        let (month_ms, year_ms) = conn.query_row(
            "SELECT COALESCE(SUM(CASE WHEN substr(day,1,7)=strftime('%Y-%m','now','localtime') THEN listened_ms ELSE 0 END),0),
             COALESCE(SUM(CASE WHEN substr(day,1,4)=strftime('%Y','now','localtime') THEN listened_ms ELSE 0 END),0) FROM listening_days", [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut query = conn.prepare(
            "SELECT t.id,t.title,t.artist,COUNT(*) FROM listening_sessions s JOIN tracks t ON t.id=s.track_id
             WHERE s.listened_ms>=MIN(10000,MAX(1,COALESCE(t.duration_ms,20000)/2))
             GROUP BY t.id ORDER BY COUNT(*) DESC,SUM(s.listened_ms) DESC,t.id LIMIT 3",
        )?;
        let top = query
            .query_map([], |r| {
                Ok(TopTrack {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    artist: r.get(2)?,
                    plays: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ListeningSummary {
            month_ms,
            year_ms,
            top,
        })
    }
}

/// Excludes paused time and seek jumps; natural endings may reset rodio's position.
#[derive(Default)]
pub struct ListeningMeter {
    position_ms: u64,
}
impl ListeningMeter {
    pub fn reset_position(&mut self, position_ms: u64) {
        self.position_ms = position_ms;
    }
    pub fn sample(
        &mut self,
        position_ms: u64,
        elapsed_ms: u64,
        paused: bool,
        ended: bool,
        duration_ms: Option<u64>,
    ) -> u64 {
        let advance = if ended {
            duration_ms
                .map(|duration| duration.saturating_sub(self.position_ms))
                .unwrap_or(elapsed_ms)
        } else {
            position_ms.saturating_sub(self.position_ms)
        };
        self.position_ms = position_ms;
        if paused { 0 } else { advance.min(elapsed_ms) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{models::NewTrack, repositories::TrackRepository};
    #[test]
    fn meter_ignores_pauses_and_seeks_and_limits_natural_end() {
        let mut meter = ListeningMeter::default();
        assert_eq!(meter.sample(500, 500, false, false, Some(1000)), 500);
        assert_eq!(meter.sample(500, 500, true, false, Some(1000)), 0);
        meter.reset_position(900);
        assert_eq!(meter.sample(0, 500, false, true, Some(1000)), 100);
        meter.reset_position(0);
        assert_eq!(meter.sample(80000, 100, false, false, None), 100);
    }
    #[test]
    fn statistics_persist_rank_sessions_filter_calendar_and_keep_deleted_time() {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(root.path()).unwrap();
        let repo = TrackRepository::new(db.clone());
        let id = repo
            .insert(&NewTrack {
                title: "Track".into(),
                artist: "Artist".into(),
                source_url: None,
                file_path: "audio/test.wav".into(),
                duration_ms: Some(20000),
            })
            .unwrap()
            .id;
        let stats = StatisticsService::new(db.clone());
        let now: i64 = db
            .connect()
            .unwrap()
            .query_row("SELECT unixepoch('now')", [], |r| r.get(0))
            .unwrap();
        stats.record("one", id, 5000, now).unwrap();
        assert!(stats.summary().unwrap().top.is_empty());
        stats.record("one", id, 6000, now).unwrap();
        stats.record("two", id, 10000, now).unwrap();
        stats.record("old", id, 10000, 946684800).unwrap();
        let summary = StatisticsService::new(Database::open(root.path()).unwrap())
            .summary()
            .unwrap();
        assert_eq!(summary.month_ms, 21000);
        assert_eq!(summary.year_ms, 21000);
        assert_eq!(summary.top[0].plays, 3);
        repo.delete(id).unwrap();
        let summary = stats.summary().unwrap();
        assert_eq!(summary.month_ms, 21000);
        assert!(summary.top.is_empty());
    }
}
