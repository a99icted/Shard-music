//! Latency regression with an intentionally occupied SQLite writer. Uses only silent fixtures.
#[allow(dead_code)]
#[path = "../src/player.rs"]
mod player;
use anyhow::{Result, ensure};
use player::{PlaybackTrack, Player};
use shard::{db::Database, models::NewTrack, repositories::TrackRepository};
use std::time::{Duration, Instant};

fn wait(player: &Player, test: impl Fn(&player::PlaybackState) -> bool) -> Result<Duration> {
    let start = Instant::now();
    loop {
        let state = player.state();
        ensure!(state.error.is_none(), "Player error: {:?}", state.error);
        if test(&state) {
            return Ok(start.elapsed());
        }
        ensure!(
            start.elapsed() < Duration::from_secs(5),
            "Player did not respond"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn main() -> Result<()> {
    let root = tempfile::tempdir()?;
    let db = Database::open(root.path())?;
    let path = root.path().join("audio/silence.wav");
    let data_len = 320_000u32;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&(data_len + 36).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    for n in [1u16, 1] {
        bytes.extend_from_slice(&n.to_le_bytes());
    }
    for n in [8000u32, 16000] {
        bytes.extend_from_slice(&n.to_le_bytes());
    }
    for n in [2u16, 16] {
        bytes.extend_from_slice(&n.to_le_bytes());
    }
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    bytes.resize(44 + data_len as usize, 0);
    std::fs::write(&path, bytes)?;
    let repo = TrackRepository::new(db.clone());
    let first = repo.insert(&NewTrack {
        title: "One".into(),
        artist: String::new(),
        source_url: None,
        file_path: "audio/silence.wav".into(),
        duration_ms: Some(20000),
    })?;
    let second = repo.insert(&NewTrack {
        title: "Two".into(),
        artist: String::new(),
        source_url: None,
        file_path: "audio/second.wav".into(),
        duration_ms: Some(20000),
    })?;
    let player = Player::new(0., false, Some(db.clone()));
    let changes = player.subscribe_changes();
    player.start(
        vec![
            PlaybackTrack {
                id: first.id,
                path: path.clone(),
            },
            PlaybackTrack {
                id: second.id,
                path,
            },
        ],
        first.id,
    );
    wait(&player, |s| {
        s.track_id == Some(first.id) && s.position_ms >= 200
    })?;
    let lock = db.connect()?;
    while changes.try_recv().is_ok() {}
    lock.execute_batch("BEGIN IMMEDIATE")?;
    let unlock = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(800));
        lock.execute_batch("COMMIT").unwrap();
    });
    player.toggle();
    let pause = wait(&player, |s| s.paused)?;
    ensure!(
        changes.try_recv().is_ok(),
        "Pause did not publish an immediate UI update"
    );
    player.next();
    let next = wait(&player, |s| s.track_id == Some(second.id))?;
    ensure!(
        changes.try_recv().is_ok(),
        "Track switch did not publish an immediate UI update"
    );
    println!("With SQLite write lock: pause={pause:?}, next={next:?}");
    unlock.join().unwrap();
    ensure!(
        pause < Duration::from_millis(250),
        "Pause was blocked by SQLite: {pause:?}"
    );
    ensure!(
        next < Duration::from_millis(250),
        "Track switch was blocked by SQLite: {next:?}"
    );
    player.stop();
    player.flush_and_wait();
    ensure!(
        shard::statistics::StatisticsService::new(db)
            .summary()?
            .year_ms
            > 0,
        "Statistics were lost"
    );
    Ok(())
}
