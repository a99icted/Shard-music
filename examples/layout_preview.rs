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

    pub fn show_ignored(app: &mut MusicApp) {
        app.home_open = false;
        app.settings_open = true;
        app.settings_category = settings::SettingsCategory::Ignoring;
    }

    pub fn show_devices(app: &mut MusicApp, code: &str, cx: &mut Context<MusicApp>) {
        app.start_sync(cx);
        app.sync_service.as_ref().unwrap().pair(code).unwrap();
        app.home_open = false;
        app.settings_open = true;
        app.settings_category = settings::SettingsCategory::ConnectedDevices;
    }

    // Regression: old libraries must appear before thumbnail migration finishes,
    // including when one cover is corrupt. Navigation must remain independent.
    pub fn check_startup_artwork(
        app: &mut MusicApp,
        count: usize,
        window: &mut Window,
        cx: &mut Context<MusicApp>,
    ) {
        app.home_open = false;
        app.selected_playlist = None;
        app.refresh(cx);
        cx.spawn_in(window, async move |this, cx| {
            let started = Instant::now();
            loop {
                cx.background_executor().timer(Duration::from_millis(10)).await;
                let ready = this.read_with(cx, |app, _| !app.library_loading).unwrap();
                assert!(started.elapsed() < Duration::from_secs(60), "Library metadata did not load");
                if ready { break; }
            }
            this.update_in(cx, |app, _, cx| {
                assert_eq!(app.tracks.len(), count);
                assert_eq!(app.visible_tracks.len(), count);
                assert!(app.active_background().is_some(), "Background metadata is missing");
                assert!(!app.thumbnail_pending.is_empty(), "Migration finished before the startup check could run");
                println!("Library ready in {:.2}s: {} tracks, background available, {} previews still pending", started.elapsed().as_secs_f64(), app.tracks.len(), app.thumbnail_pending.len());
                app.selected_playlist = app.playlists.iter().find(|p| p.id != shard::favorites::FAVORITES_ID && p.track_count == count as i64).map(|p| p.id);
                assert!(app.selected_playlist.is_some());
                app.refresh(cx);
            }).unwrap();
            loop {
                cx.background_executor().timer(Duration::from_millis(20)).await;
                let ready = this.read_with(cx, |app, _| !app.library_loading).unwrap();
                assert!(started.elapsed() < Duration::from_secs(60), "Navigation did not finish");
                if ready { break; }
            }
            this.read_with(cx, |app, _| {
                assert_eq!(app.visible_tracks.len(), count);
                assert!(app.active_background().is_some());
                println!("Playlist navigation passed during background migration");
            }).unwrap();
            loop {
                cx.background_executor().timer(Duration::from_millis(100)).await;
                let done = this.read_with(cx, |app, _| app.thumbnail_cancel.is_none()).unwrap();
                assert!(started.elapsed() < Duration::from_secs(60), "Preview migration did not finish");
                if done { break; }
            }
            this.read_with(cx, |app, _| {
                let ready = app.tracks.iter().filter(|track| app.preview_paths.contains(&shard::artwork::track_path(&app.data_directory, track))).count();
                assert_eq!(ready, count - 1, "Valid previews must appear; the corrupt cover must stay a placeholder");
                assert!(app.thumbnail_queue.is_empty() && app.thumbnail_pending.is_empty());
                assert!(app.active_background().is_some());
                println!("Startup artwork regression passed: {ready} previews, one corrupt cover isolated, tracks and background retained");
            }).unwrap();
            let _ = cx.update(|_, cx| cx.quit());
        }).detach();
    }

    // Exercise cache eviction using the real virtual list and temporary artwork.
    pub fn stress_artwork(app: &mut MusicApp, window: &mut Window, cx: &mut Context<MusicApp>) {
        app.home_open = false;
        app.selected_playlist = None;
        app.refresh(cx);
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if this
                    .read_with(cx, |app, _| {
                        !app.library_loading && app.thumbnail_cancel.is_none()
                    })
                    .unwrap_or(true)
                {
                    break;
                }
            }
            let count = this.read_with(cx, |app, _| app.tracks.len()).unwrap_or(0);
            for pass in 0..3 {
                for index in (0..count).step_by(6) {
                    let _ = this.update_in(cx, |app, _, cx| {
                        app.collection_list.scroll_to(ListOffset {
                            item_ix: index + 1,
                            offset_in_item: px(0.),
                        });
                        cx.notify();
                    });
                    cx.background_executor()
                        .timer(Duration::from_millis(120))
                        .await;
                }
                let _ = this.update_in(cx, |app, window, cx| {
                    app.images
                        .update(cx, |cache, cx| cache.begin_frame(window, cx));
                    let (images, bytes) = app.images.update(cx, |cache, _| cache.diagnostics());
                    println!(
                        "Artwork pass {}: {images} cached images, {:.2} MiB decoded",
                        pass + 1,
                        bytes as f64 / 1048576.
                    );
                    assert!(
                        images <= 96 && bytes <= 32 * 1024 * 1024,
                        "Artwork cache exceeded its budget"
                    );
                });
            }
            println!("Artwork cache stress passed");
            let _ = cx.update(|_, cx| cx.quit());
        })
        .detach();
    }

    // Exercise real duplicate review against a second temporary library.
    pub fn prepare_sync(
        app: &mut MusicApp,
        peer_code: &str,
        window: &mut Window,
        cx: &mut Context<MusicApp>,
    ) -> anyhow::Result<()> {
        app.start_sync(cx);
        app.sync_service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Preview synchronization did not start"))?
            .pair(peer_code)?;
        app.request_sync(shard::sync::SyncSelection::Library, window, cx);
        Ok(())
    }

    // Deterministic UI fixture; uses the same worker, progress channel and cancellation
    // path as an import, without network access or changes to the user's library.
    pub fn simulate_download(app: &mut MusicApp, cx: &mut Context<MusicApp>) {
        app.launch_download(
            "https://youtube.com/watch?v=preview".into(),
            |_, _, cancel, sender| {
                for completed in 0..12 {
                    sender.send(YoutubeProgress {
                        completed,
                        total: 12,
                        current_title: Some(format!("Тестовый трек {}", completed + 1)),
                    })?;
                    for _ in 0..100 {
                        if cancel.load(Ordering::Relaxed) {
                            return Ok(shard::youtube::YoutubeDownloadReport {
                                cancelled: true,
                                ..Default::default()
                            });
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                Ok(shard::youtube::YoutubeDownloadReport::default())
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
    let show_ignored = options.iter().any(|arg| arg == "--ignored");
    let show_devices = options.iter().any(|arg| arg == "--devices");
    let simulate_download = options.iter().any(|arg| arg == "--download");
    let simulate_sync = options.iter().any(|arg| arg == "--sync");
    let stress_artwork = options.iter().any(|arg| arg == "--artwork-stress");
    let check_startup_artwork = options.iter().any(|arg| arg == "--startup-artwork-check");
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
    for index in 0..6 {
        playlists.create(&format!("Тестовая подборка {}", index + 1))?;
    }
    let cover_path = root.path().join("preview-cover.png");
    image::RgbImage::from_fn(480, 480, |x, y| {
        image::Rgb([(40 + x * 120 / 480) as u8, (50 + y * 80 / 480) as u8, 190])
    })
    .save(&cover_path)?;
    shard::appearance::AppearanceService::new(db.clone()).set_cover(playlist, &cover_path)?;
    if check_startup_artwork {
        anyhow::ensure!(
            track_count > 16,
            "Startup regression needs more than two thumbnail batches"
        );
        shard::backgrounds::BackgroundService::new(db.clone()).set_image(0, &cover_path)?;
    }
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
        std::fs::create_dir_all(root.path().join("artwork"))?;
        std::fs::copy(&cover_path, shard::artwork::track_path(root.path(), &track))?;
        if check_startup_artwork && index == track_count {
            std::fs::write(
                shard::artwork::track_path(root.path(), &track),
                b"broken image",
            )?;
        }
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
    let receiver_root = tempfile::tempdir()?;
    let receiver = if simulate_sync || show_devices {
        let receiver_db = Database::open(receiver_root.path())?;
        shard::services::TrackService::new(receiver_db.clone()).import_local(&silent_audio)?;
        Some(shard::sync::SyncService::start(
            receiver_db,
            uuid::Uuid::new_v4().to_string(),
            "Preview receiver".into(),
        )?)
    } else {
        None
    };
    let peer_code = receiver.as_ref().map(|service| service.code().to_owned());
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
                        if show_ignored {
                            let ignored = shard::ignored::IgnoredTracksService::new(db.clone());
                            for track in shard::services::TrackService::new(db.clone())
                                .list_tracks()
                                .unwrap()
                                .into_iter()
                                .take(3)
                            {
                                ignored.set(track.id, true).unwrap();
                            }
                        }
                        let mut app = ui::MusicApp::new(db, window, cx);
                        if show_ignored {
                            ui::show_ignored(&mut app);
                        }
                        if simulate_download {
                            ui::simulate_download(&mut app, cx);
                        }
                        if stress_artwork {
                            ui::stress_artwork(&mut app, window, cx);
                        }
                        if check_startup_artwork {
                            ui::check_startup_artwork(&mut app, track_count, window, cx);
                        }
                        if let Some(code) = &peer_code {
                            if show_devices {
                                ui::show_devices(&mut app, code, cx);
                            } else {
                                ui::prepare_sync(&mut app, code, window, cx)
                                    .expect("prepare sync preview");
                            }
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
    if let Some(receiver) = receiver {
        receiver.stop();
    }
    drop(root);
    Ok(())
}
