//! Isolated layout preview: cargo run --example layout_preview -- 760 520
//! Uses a temporary library with silent audio; never opens the user's library.
#[cfg(windows)]
#[path = "../src/desktop.rs"]
mod desktop;
#[path = "../src/input.rs"]
mod input;
#[path = "../src/player.rs"]
mod player;
mod ui {
    include!("../src/ui.rs");

    // Deterministic UI fixture; uses the same worker, progress channel and cancellation
    // path as an import, without network access or changes to the user's library.
    pub fn simulate_download(app: &mut MusicApp, cx: &mut Context<MusicApp>) {
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        app.download_progress = Some(receiver);
        app.download_cancel = Some(cancel.clone());
        app.modal = Some(Modal::Import);
        app.run_job(
            "Тестовый прогресс загрузки…",
            move || {
                for completed in 0..12 {
                    sender.send(YoutubeProgress {
                        completed,
                        total: 12,
                        current_title: Some(format!("Тестовый трек {}", completed + 1)),
                    })?;
                    for _ in 0..100 {
                        if cancel.load(Ordering::Relaxed) {
                            return Ok(());
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                Ok(())
            },
            |app, (), _| {
                if matches!(app.modal, Some(Modal::Import)) {
                    app.modal = None;
                }
                app.message = Some("Тестовый прогресс завершён; файлы не скачивались".into());
            },
            cx,
        );
    }
}

use anyhow::Result;
use gpui::{App, AppContext, Application, Bounds, WindowBounds, WindowOptions, px, size};
use shard::{db::Database, models::NewTrack, preferences::Preferences, repositories::*};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let width: f32 = args.next().unwrap_or("960".into()).parse()?;
    let height: f32 = args.next().unwrap_or("640".into()).parse()?;
    let options: Vec<_> = args.collect();
    let simulate_download = options.iter().any(|arg| arg == "--download");
    #[cfg(windows)]
    let taskbar = options.iter().any(|arg| arg == "--taskbar");
    let track_count: usize = options
        .iter()
        .find_map(|arg| arg.strip_prefix("--tracks="))
        .unwrap_or("12")
        .parse()?;
    let root = tempfile::tempdir()?;
    let db = Database::open(root.path())?;
    Preferences {
        volume: 0.,
        queue_visible: false,
        ..Default::default()
    }
    .save(root.path())?;
    let tracks = TrackRepository::new(db.clone());
    let playlists = PlaylistRepository::new(db.clone());
    let playlist = playlists.create("Вечерняя музыка — длинное название плейлиста")?;
    let other_playlists = [
        playlists.create("Дорога домой")?,
        playlists.create("Сосредоточиться")?,
        playlists.create("Пустой плейлист")?,
    ];
    let cover_path = root.path().join("preview-cover.png");
    image::RgbImage::from_fn(480, 480, |x, y| {
        image::Rgb([(40 + x * 120 / 480) as u8, (50 + y * 80 / 480) as u8, 190])
    })
    .save(&cover_path)?;
    shard::appearance::AppearanceService::new(db.clone()).set_cover(playlist, &cover_path)?;
    println!("Preview cover: {}", cover_path.display());
    // Twenty seconds of silence, sufficient for inspecting active playback.
    let samples = vec![0u8; 8000 * 2 * 20];
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&8000u32.to_le_bytes());
    wav.extend_from_slice(&16000u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    wav.extend_from_slice(&samples);
    let silent_audio = root.path().join("silent.wav");
    std::fs::write(&silent_audio, &wav)?;
    for index in 1..=track_count {
        let path = format!("audio/{index}.wav");
        std::fs::hard_link(&silent_audio, root.path().join(&path))?;
        let track = tracks.insert(&NewTrack {
            title: format!("{index:02} · Длинное название трека — вечерний город 🎵"),
            artist: "Исполнитель / Artist".into(),
            source_url: None,
            file_path: path,
            duration_ms: Some(20_000),
        })?;
        playlists.add_track(playlist, track.id)?;
        if index <= 6 {
            playlists.add_track(other_playlists[0], track.id)?;
        }
        if index <= 3 {
            playlists.add_track(other_playlists[1], track.id)?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs() as i64;
            for play in 0..(4 - index) {
                shard::statistics::StatisticsService::new(db.clone()).record(
                    &format!("preview-{index}-{play}"),
                    track.id,
                    20000,
                    now,
                )?;
            }
        }
    }
    Application::new()
        .with_assets(shard::branding::Assets)
        .run(move |cx: &mut App| {
            input::bind_keys(cx);
            ui::bind_keys(cx);
            let bounds =
                Bounds::centered(None, size(px(width.max(760.)), px(height.max(520.))), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(760.), px(520.))),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Shard — Layout preview".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| {
                        let mut app = ui::MusicApp::new(db, window, cx);
                        if simulate_download {
                            ui::simulate_download(&mut app, cx);
                        }
                        app
                    });
                    #[cfg(windows)]
                    if taskbar {
                        desktop::install(window, cx, view.read(cx).player())
                            .expect("desktop integration");
                    }
                    view
                },
            )
            .expect("open preview");
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
    drop(root);
    Ok(())
}
