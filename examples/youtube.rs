use anyhow::{Context, Result};
use shard::{
    db::Database,
    download_tools::DownloadTools,
    services::{TrackService, application_directory},
};
use std::sync::atomic::AtomicBool;

fn main() -> Result<()> {
    let root = application_directory()?;
    let argument = std::env::args().nth(1).context(
        "Usage: cargo run --no-default-features --example youtube -- <YouTube or Spotify URL | --check-tools>",
    )?;
    if argument == "--check-tools" {
        let tools = DownloadTools::discover(&root)?;
        println!(
            "yt-dlp: {}\nFFmpeg: {}",
            tools.yt_dlp.display(),
            tools.ffmpeg_directory.display()
        );
        return Ok(());
    }
    let db = Database::open(root)?;
    let service = TrackService::new(db);
    let report = service.download_collection(&argument, &AtomicBool::new(false), |progress| {
        println!("{}", progress.message())
    })?;
    for track in &report.tracks {
        println!(
            "Imported #{}: {} — {}\n{}",
            track.id,
            track.artist,
            track.title,
            service.audio_path(track)?.display()
        );
    }
    println!("{}", report.message());
    if let Some(errors) = report.error_details() {
        eprintln!("{errors}");
    }
    anyhow::ensure!(
        !report.tracks.is_empty(),
        "Не удалось загрузить ни одного трека"
    );
    Ok(())
}
