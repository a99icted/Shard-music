//! Hardware smoke check using generated silent WAVs; does not touch the music library.
#[allow(dead_code)]
#[path = "../src/player.rs"]
mod player;

use anyhow::{Result, ensure};
use player::{PlaybackTrack, Player};
use std::{
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

fn silence(path: &Path) -> Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&16036u32.to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&8000u32.to_le_bytes())?;
    file.write_all(&16000u32.to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&16000u32.to_le_bytes())?;
    file.write_all(&vec![0; 16000])?;
    Ok(())
}

fn wait(player: &Player, predicate: impl Fn(&player::PlaybackState) -> bool) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = player.state();
        ensure!(state.error.is_none(), "Playback failed: {:?}", state.error);
        if predicate(&state) {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "Timed out: current={:?}, paused={}",
            state.track_id,
            state.paused
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn main() -> Result<()> {
    if let Some(path) = std::env::args_os().nth(1) {
        let player = Player::new(0., false, None);
        player.start(
            vec![PlaybackTrack {
                id: 1,
                path: path.into(),
            }],
            1,
        );
        wait(&player, |state| state.track_id == Some(1))?;
        wait(&player, |state| state.position_ms >= 1000)?;
        player.seek(5000);
        wait(&player, |state| state.position_ms >= 5000)?;
        player.stop();
        wait(&player, |state| state.track_id.is_none())?;
        println!("Real audio playback and seeking passed.");
        return Ok(());
    }
    let root = tempfile::tempdir()?;
    let wav = root.path().join("silence.wav");
    silence(&wav)?;
    let player = Player::new(0., false, None);
    let transport = player.subscribe_transport();
    ensure!(
        !transport.try_recv()?.has_track,
        "Initial transport must be idle"
    );
    player.start(
        (1..=3)
            .map(|id| PlaybackTrack {
                id,
                path: wav.clone(),
            })
            .collect(),
        1,
    );
    wait(&player, |state| state.track_id == Some(1))?;
    ensure!(transport.try_recv()?.has_track, "Start was not published");
    wait(&player, |state| state.position_ms >= 150)?;
    ensure!(
        transport.is_empty(),
        "Position ticks must not wake taskbar controls"
    );
    wait(&player, |state| state.track_id == Some(2))?;
    player.toggle();
    wait(&player, |state| state.paused)?;
    ensure!(transport.try_recv()?.paused, "Pause was not published");
    player.set_volume(0.25);
    player.set_shuffle(true);
    wait(&player, |state| state.queue.shuffled())?;
    player.select(3);
    wait(&player, |state| state.track_id == Some(3))?;
    player.seek(500);
    wait(&player, |state| state.position_ms >= 500)?;
    player.remove_and_wait(3)?;
    ensure!(
        player.state().track_id.is_none(),
        "Removing last track did not stop playback"
    );
    player.enqueue(PlaybackTrack {
        id: 4,
        path: wav.clone(),
    });
    wait(&player, |state| state.track_id == Some(4))?;
    player.stop();
    wait(&player, |state| state.track_id.is_none())?;
    drop(player);
    std::thread::sleep(Duration::from_millis(200));
    let db = shard::db::Database::open(root.path())?;
    let repository = shard::repositories::TrackRepository::new(db.clone());
    let track = repository.insert(&shard::models::NewTrack {
        title: "Repeat fixture".into(),
        artist: String::new(),
        source_url: None,
        file_path: "silence.wav".into(),
        duration_ms: Some(1000),
    })?;
    let player = Player::new(0., false, Some(db.clone()));
    player.set_repeat(shard::queue::RepeatMode::One);
    player.start(
        vec![PlaybackTrack {
            id: track.id,
            path: wav.clone(),
        }],
        track.id,
    );
    wait(&player, |s| s.track_id == Some(track.id))?;
    std::thread::sleep(Duration::from_millis(2400));
    ensure!(
        player.state().track_id == Some(track.id),
        "Repeat one stopped at the end"
    );
    player.toggle();
    wait(&player, |s| s.paused)?;
    let stats = shard::statistics::StatisticsService::new(db);
    let before = stats.summary()?.year_ms;
    ensure!(
        (1500..=3000).contains(&before),
        "Unexpected listening time: {before}"
    );
    std::thread::sleep(Duration::from_millis(500));
    ensure!(
        stats.summary()?.year_ms == before,
        "Pause counted as listening"
    );
    player.seek(800);
    std::thread::sleep(Duration::from_millis(150));
    ensure!(
        stats.summary()?.year_ms == before,
        "Seek counted as listening"
    );
    player.set_repeat(shard::queue::RepeatMode::Off);
    player.toggle();
    wait(&player, |s| s.track_id.is_none())?;
    ensure!(
        stats.summary()?.top[0].plays >= 2,
        "Repeat sessions did not count separately"
    );
    player.set_repeat(shard::queue::RepeatMode::All);
    player.start(
        vec![PlaybackTrack {
            id: track.id,
            path: wav,
        }],
        track.id,
    );
    wait(&player, |s| s.track_id == Some(track.id))?;
    std::thread::sleep(Duration::from_millis(1300));
    ensure!(
        player.state().track_id == Some(track.id),
        "Repeat all stopped at the end"
    );
    player.stop();
    wait(&player, |s| s.track_id.is_none())?;
    player.start(
        vec![PlaybackTrack {
            id: track.id,
            path: root.path().join("missing.wav"),
        }],
        track.id,
    );
    std::thread::sleep(Duration::from_millis(200));
    ensure!(
        player.state().track_id.is_none(),
        "Invalid audio loops with repeat enabled"
    );
    player.take_error();
    drop(player);
    std::thread::sleep(Duration::from_millis(200));
    println!(
        "Audio smoke: playback, repeat one/all, invalid audio, persisted listening time, pause and seek accounting passed."
    );
    Ok(())
}
