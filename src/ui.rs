#[path = "ui/components.rs"]
mod components;
#[path = "ui/home.rs"]
mod home;
#[path = "ui/settings.rs"]
mod settings;

use crate::{
    input::TextInput,
    player::{PlaybackTrack, Player},
};
use anyhow::Result;
use gpui::{prelude::*, *};
use shard::{
    appearance::AppearanceService,
    backgrounds::{Background, BackgroundService},
    db::Database,
    models::{Playlist, Track},
    preferences::{Preferences, PreferencesWriter},
    queue::RepeatMode,
    services::{AUDIO_EXTENSIONS, PlaylistService, TrackService},
    statistics::{ListeningSummary, StatisticsService},
    theme::{Palette, Theme},
    youtube::YoutubeProgress,
};
use std::{
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

actions!(
    shard,
    [
        Dismiss,
        Submit,
        TogglePlayback,
        NextTrack,
        PreviousTrack,
        ToggleQueue,
        VolumeUp,
        VolumeDown
    ]
);
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, None),
        KeyBinding::new("enter", Submit, None),
        KeyBinding::new("space", TogglePlayback, Some("MusicApp && !TextInput")),
        KeyBinding::new("ctrl-right", NextTrack, Some("MusicApp && !TextInput")),
        KeyBinding::new("ctrl-left", PreviousTrack, Some("MusicApp && !TextInput")),
        KeyBinding::new("ctrl-q", ToggleQueue, Some("MusicApp && !TextInput")),
        KeyBinding::new("ctrl-up", VolumeUp, Some("MusicApp && !TextInput")),
        KeyBinding::new("ctrl-down", VolumeDown, Some("MusicApp && !TextInput")),
    ]);
}

#[derive(Clone)]
enum Modal {
    PlaylistAppearance(i64),
    Import,
    CreatePlaylist,
    AddToPlaylist(Track),
    PickTracks(i64),
    TrackActions(Track),
    DeleteTrack(Track),
    DeletePlaylist(Playlist),
}

pub(crate) struct UiTheme(pub Theme);
impl Global for UiTheme {}

pub struct MusicApp {
    settings_open: bool,
    background_service: BackgroundService,
    backgrounds: Vec<Background>,
    blur_preview: Option<u8>,
    collection_list: ListState,
    collection_tracks: Rc<Vec<Track>>,
    collection_dirty: bool,
    collection_items: Vec<i64>,
    collection_query: String,
    scroll_context: Option<i64>,
    home_open: bool,
    statistics: StatisticsService,
    listening: ListeningSummary,
    statistics_updated: Instant,
    statistics_loading: bool,
    library_generation: u64,
    loaded_playlist: Option<i64>,
    appearance_service: AppearanceService,
    job_progress: Option<YoutubeProgress>,
    tracks_service: TrackService,
    playlists_service: PlaylistService,
    tracks: Vec<Track>,
    track_index: std::collections::HashMap<i64, usize>,
    playlists: Vec<Playlist>,
    visible_tracks: Vec<Track>,
    selected_playlist: Option<i64>,
    modal: Option<Modal>,
    input: Entity<TextInput>,
    search: Entity<TextInput>,
    focus: FocusHandle,
    busy: Option<String>,
    message: Option<String>,
    error: Option<String>,
    player: Player,
    download_progress: Option<mpsc::Receiver<YoutubeProgress>>,
    download_cancel: Option<Arc<AtomicBool>>,
    preferences: Preferences,
    preferences_writer: PreferencesWriter,
    data_directory: PathBuf,
    error_details_open: bool,
    playback_context: String,
    playback_playlist: Option<i64>,
    playback_query: String,
    slider_bounds: [Option<Bounds<Pixels>>; 3],
    dragging_slider: Option<components::SliderKind>,
    seek_preview: Option<u64>,
    last_volume: f32,
}

impl MusicApp {
    pub fn player(&self) -> Player {
        self.player.clone()
    }

    fn colors(&self) -> Palette {
        self.preferences.theme.palette()
    }

    fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.preferences.theme = theme;
        cx.set_global(UiTheme(theme));
        self.search.update(cx, |_, cx| cx.notify());
        self.input.update(cx, |_, cx| cx.notify());
        self.save_preferences();
        cx.notify();
    }

    pub fn new(db: Database, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| TextInput::new("Поиск по названию или исполнителю", cx));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        let focus = cx.focus_handle();
        let data_directory = db.root().to_path_buf();
        let preferences = Preferences::load(&data_directory);
        let player = Player::new(preferences.volume, preferences.shuffle, Some(db.clone()));
        player.set_repeat(preferences.repeat);
        cx.set_global(UiTheme(preferences.theme));
        window.focus(&focus);
        let mut app = Self {
            settings_open: false,
            background_service: BackgroundService::new(db.clone()),
            backgrounds: vec![],
            blur_preview: None,
            collection_list: ListState::new(1, ListAlignment::Top, px(256.)),
            collection_tracks: Rc::new(vec![]),
            collection_dirty: true,
            collection_items: vec![],
            collection_query: String::new(),
            scroll_context: None,
            home_open: true,
            statistics: StatisticsService::new(db.clone()),
            listening: ListeningSummary::default(),
            statistics_updated: Instant::now(),
            statistics_loading: false,
            library_generation: 0,
            loaded_playlist: None,
            appearance_service: AppearanceService::new(db.clone()),
            job_progress: None,
            tracks_service: TrackService::new(db.clone()),
            playlists_service: PlaylistService::new(db),
            tracks: vec![],
            track_index: std::collections::HashMap::new(),
            playlists: vec![],
            visible_tracks: vec![],
            selected_playlist: None,
            modal: None,
            input: cx.new(|cx| TextInput::new("", cx)),
            search,
            focus,
            busy: None,
            message: None,
            error: None,
            player,
            download_progress: None,
            download_cancel: None,
            last_volume: if preferences.volume > 0. {
                preferences.volume
            } else {
                0.7
            },
            preferences_writer: PreferencesWriter::new(data_directory.clone()),
            preferences,
            data_directory,
            error_details_open: false,
            playback_context: "Ваша библиотека".into(),
            playback_playlist: None,
            playback_query: String::new(),
            slider_bounds: [None, None, None],
            dragging_slider: None,
            seek_preview: None,
        };
        app.refresh(cx);
        cx.on_release(|app, _| {
            app.player.flush_and_wait();
            app.preferences_writer.flush_and_wait();
        })
        .detach();
        cx.on_app_quit(|app, _| {
            if let Some(cancel) = &app.download_cancel {
                cancel.store(true, Ordering::Relaxed);
            }
            app.player.stop();
            app.player.flush_and_wait();
            app.preferences_writer.flush_and_wait();
            async {}
        })
        .detach();
        let changes = app.player.subscribe_changes();
        cx.spawn_in(window, async move |this, cx| {
            while changes.recv().await.is_ok() {
                if this.update_in(cx, |_, _, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.spawn_in(window, async move |this, cx| {
            let mut last_player = None;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                if this
                    .update_in(cx, |app, window, cx| {
                        let snapshot = app.player.progress();
                        let mut changed = last_player != Some(snapshot);
                        last_player = Some(snapshot);
                        if window.is_window_active()
                            && app.home_open
                            && app.statistics_updated.elapsed() >= Duration::from_secs(5)
                        {
                            app.refresh_statistics(cx);
                            changed = true;
                        }
                        if let Some(error) = app.player.take_error() {
                            app.error = Some(error);
                            app.error_details_open = false;
                            changed = true;
                        }
                        if let Some(error) = app.preferences_writer.take_error() {
                            app.error = Some(error);
                            changed = true;
                        }
                        if let Some(progress) = app
                            .download_progress
                            .as_ref()
                            .and_then(|receiver| receiver.try_iter().last())
                        {
                            app.busy = Some(progress.message());
                            app.job_progress = Some(progress.clone());
                            changed = true;
                            if progress.completed > 0 {
                                app.refresh(cx);
                            }
                        }
                        if changed && window.is_window_active() {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        app
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.home_open {
            self.refresh_statistics(cx);
        }
        self.library_generation += 1;
        let generation = self.library_generation;
        let selected = self.selected_playlist;
        if self.loaded_playlist != selected {
            self.visible_tracks.clear();
            self.collection_dirty = true;
        }
        let tracks = self.tracks_service.clone();
        let playlists = self.playlists_service.clone();
        let backgrounds = self.background_service.clone();
        let task = cx.background_executor().spawn(async move {
            let tracks = tracks.list_tracks()?;
            let visible = match selected {
                Some(id) => playlists.list_playlist_tracks(id)?,
                None => tracks.clone(),
            };
            Ok::<_, anyhow::Error>((
                tracks,
                playlists.list_playlists()?,
                visible,
                backgrounds.list()?,
            ))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                // A slower previous request must not overwrite a newer navigation or import.
                if app.library_generation != generation {
                    return;
                }
                match result {
                    Ok((tracks, playlists, visible, backgrounds)) => {
                        app.track_index = tracks
                            .iter()
                            .enumerate()
                            .map(|(index, track)| (track.id, index))
                            .collect();
                        app.tracks = tracks;
                        app.playlists = playlists;
                        app.visible_tracks = visible;
                        app.backgrounds = backgrounds;
                        app.loaded_playlist = selected;
                        app.collection_dirty = true;
                    }
                    Err(error) => app.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn run_job<T: Send + 'static>(
        &mut self,
        label: &str,
        job: impl FnOnce() -> Result<T> + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some(label.to_owned());
        self.job_progress = None;
        self.error = None;
        self.error_details_open = false;
        self.message = None;
        let task = cx.background_executor().spawn(async move { job() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                app.busy = None;
                app.download_progress = None;
                app.download_cancel = None;
                app.job_progress = None;
                match result {
                    Ok(value) => done(app, value, cx),
                    Err(error) => app.error = Some(format!("{error:#}")),
                }
                app.refresh(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn open_modal(&mut self, modal: Modal, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() && matches!(modal, Modal::Import) {
            self.modal = Some(Modal::Import);
            window.focus(&self.focus);
            cx.notify();
            return;
        }
        let has_input = matches!(
            modal,
            Modal::Import | Modal::CreatePlaylist | Modal::PlaylistAppearance(_)
        );
        let placeholder = match modal {
            Modal::Import => "Ссылка на трек или плейлист YouTube",
            Modal::CreatePlaylist => "Например, Вечерний маршрут",
            Modal::PlaylistAppearance(_) => "Свой цвет: #34465E",
            _ => "",
        };
        self.input = cx.new(|cx| TextInput::new(placeholder, cx));
        self.modal = Some(modal);
        self.error = None;
        self.message = None;
        if has_input {
            window.focus(&self.input.focus_handle(cx));
        } else {
            window.focus(&self.focus);
        }
        cx.notify();
    }

    fn dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.take().is_none() && self.preferences.queue_visible {
            self.toggle_queue(cx);
        }
        window.focus(&self.focus);
        cx.notify();
    }

    fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() && matches!(self.modal, Some(Modal::Import)) {
            return;
        }
        match self.modal.clone() {
            Some(Modal::PlaylistAppearance(id)) => {
                match shard::appearance::parse_color(&self.input.read(cx).value()) {
                    Ok(color) => self.set_playlist_color(id, Some(color), cx),
                    Err(error) => {
                        self.error = Some(format!("{error:#}"));
                        cx.notify();
                    }
                }
            }
            Some(Modal::Import) => {
                let url = self.input.read(cx).value();
                if let Err(error) = shard::services::validate_youtube_url(&url) {
                    self.error = Some(format!("{error:#}"));
                    cx.notify();
                    return;
                }
                let service = self.tracks_service.clone();
                let (sender, receiver) = mpsc::channel();
                let cancel = Arc::new(AtomicBool::new(false));
                self.download_progress = Some(receiver);
                self.download_cancel = Some(cancel.clone());
                self.run_job(
                    "Получаем список треков с YouTube…",
                    move || {
                        service.download_youtube_collection(&url, &cancel, |progress| {
                            let _ = sender.send(progress);
                        })
                    },
                    |app, report, _| {
                        let was_open = matches!(app.modal, Some(Modal::Import));
                        if was_open {
                            app.modal = None;
                        }
                        app.message = Some(report.message());
                        app.error = report.error_details();
                        if let Some(id) = report.playlist_id
                            && was_open
                        {
                            app.settings_open = false;
                            app.home_open = false;
                            app.selected_playlist = Some(id);
                        }
                    },
                    cx,
                );
                window.focus(&self.focus);
            }
            Some(Modal::CreatePlaylist) => {
                let service = self.playlists_service.clone();
                let name = self.input.read(cx).value();
                self.run_job(
                    "Создаём плейлист…",
                    move || service.create_playlist(&name),
                    |app, id, _| {
                        app.settings_open = false;
                        app.home_open = false;
                        app.selected_playlist = Some(id);
                        app.modal = None;
                        app.message =
                            Some("Плейлист создан. Добавьте в него треки из библиотеки".into());
                    },
                    cx,
                );
                window.focus(&self.focus);
            }
            _ => {}
        }
    }

    fn import_local(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let service = self.tracks_service.clone();
        self.run_job(
            "Добавляем локальные файлы…",
            move || {
                // GPUI's worker pool uses MTA; Windows file dialogs require their own STA thread.
                let paths = std::thread::Builder::new()
                    .name("shard-file-picker".into())
                    .spawn(|| {
                        rfd::FileDialog::new()
                            .set_title("Добавить музыку в Shard")
                            .add_filter("Аудиофайлы", AUDIO_EXTENSIONS)
                            .pick_files()
                            .unwrap_or_default()
                    })?
                    .join()
                    .map_err(|_| anyhow::anyhow!("Не удалось открыть окно выбора файла"))?;
                let mut imported = 0;
                let mut failures = vec![];
                for path in paths {
                    match service.import_local(&path) {
                        Ok(_) => imported += 1,
                        Err(error) => failures.push(format!(
                            "{}: {error:#}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        )),
                    }
                }
                Ok((imported, failures))
            },
            |app, (count, failures), _| {
                if matches!(app.modal, Some(Modal::Import)) {
                    app.modal = None;
                }
                if count > 0 {
                    app.message = Some(format!("Добавлено треков: {count}"));
                }
                if !failures.is_empty() {
                    app.error = Some(failures.join("\n"));
                }
            },
            cx,
        );
    }

    fn add_to_playlist(&mut self, playlist: i64, track: i64, cx: &mut Context<Self>) {
        let service = self.playlists_service.clone();
        self.run_job(
            "Добавляем трек в плейлист…",
            move || service.add_track(playlist, track),
            |app, added, _| {
                app.message = Some(
                    if added {
                        "Трек добавлен в плейлист"
                    } else {
                        "Этот трек уже есть в плейлисте"
                    }
                    .into(),
                )
            },
            cx,
        );
    }

    fn current_title(&self) -> String {
        self.selected_playlist
            .and_then(|id| self.playlists.iter().find(|p| p.id == id))
            .map(|p| p.name.clone())
            .unwrap_or("Все треки".into())
    }

    fn filtered_tracks(&self, cx: &App) -> Vec<Track> {
        let query = self.search.read(cx).value().to_lowercase();
        self.visible_tracks
            .iter()
            .filter(|track| {
                query.is_empty()
                    || track.title.to_lowercase().contains(&query)
                    || track.artist.to_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }

    fn playback_track(&self, track: &Track) -> Result<PlaybackTrack> {
        Ok(PlaybackTrack {
            id: track.id,
            path: self.tracks_service.audio_path(track)?,
        })
    }

    fn play(&mut self, track: Track, cx: &mut Context<Self>) {
        let state = self.player.state();
        let query = self.search.read(cx).value();
        if state.track_id == Some(track.id) {
            self.player.toggle();
        } else if self.playback_playlist == self.selected_playlist
            && self.playback_query == query
            && state.queue.contains(track.id)
        {
            self.player.select(track.id);
        } else {
            let result: Result<Vec<_>> = self
                .filtered_tracks(cx)
                .iter()
                .map(|track| self.playback_track(track))
                .collect();
            match result {
                Ok(tracks) => {
                    self.playback_context = self.current_title();
                    self.playback_playlist = self.selected_playlist;
                    self.playback_query = query;
                    self.player.start(tracks, track.id);
                }
                Err(error) => self.error = Some(format!("{error:#}")),
            }
        }
        cx.notify();
    }

    fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        if self.modal.is_some() {
            return;
        }
        if self.player.state().track_id.is_some() {
            self.player.toggle();
        } else if let Some(track) = self.filtered_tracks(cx).first().cloned() {
            self.play(track, cx);
        }
        cx.notify();
    }

    fn play_collection(&mut self, cx: &mut Context<Self>) {
        if self.player.state().track_id.is_some()
            && self.playback_playlist == self.selected_playlist
        {
            self.player.toggle();
        } else if let Some(track) = self.filtered_tracks(cx).first().cloned() {
            self.play(track, cx);
        }
        cx.notify();
    }

    fn save_preferences(&mut self) {
        self.preferences_writer.save(self.preferences.clone());
    }

    fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.preferences.volume = volume.clamp(0., 1.);
        if volume > 0. {
            self.last_volume = volume;
        }
        self.player.set_volume(self.preferences.volume);
        cx.notify();
    }

    fn toggle_shuffle(&mut self, cx: &mut Context<Self>) {
        self.preferences.shuffle = !self.preferences.shuffle;
        self.player.set_shuffle(self.preferences.shuffle);
        self.save_preferences();
        cx.notify();
    }

    fn cycle_repeat(&mut self, cx: &mut Context<Self>) {
        self.preferences.repeat = self.preferences.repeat.next();
        self.player.set_repeat(self.preferences.repeat);
        self.save_preferences();
        cx.notify();
    }

    fn toggle_queue(&mut self, cx: &mut Context<Self>) {
        self.preferences.queue_visible = !self.preferences.queue_visible;
        self.save_preferences();
        cx.notify();
    }

    fn choose_cover(&mut self, id: i64, cx: &mut Context<Self>) {
        let service = self.appearance_service.clone();
        self.run_job(
            "Готовим обложку…",
            move || {
                let path = std::thread::Builder::new()
                    .name("shard-cover-picker".into())
                    .spawn(|| {
                        rfd::FileDialog::new()
                            .set_title("Обложка плейлиста")
                            .add_filter("Изображения", &["png", "jpg", "jpeg", "webp"])
                            .pick_file()
                    })?
                    .join()
                    .map_err(|_| anyhow::anyhow!("Не удалось открыть выбор изображения"))?;
                if let Some(path) = path {
                    service.set_cover(id, &path)?;
                }
                Ok(())
            },
            |_, _, _| {},
            cx,
        );
    }

    fn set_playlist_color(&mut self, id: i64, color: Option<u32>, cx: &mut Context<Self>) {
        let service = self.appearance_service.clone();
        self.run_job(
            "Сохраняем цвет плейлиста…",
            move || service.set_header_color(id, color),
            |_, (), _| {},
            cx,
        );
    }

    fn enqueue_track(&mut self, track: &Track, cx: &mut Context<Self>) {
        if self.player.state().queue.contains(track.id) {
            self.message = Some("Этот трек уже есть в очереди".into());
        } else {
            match self.playback_track(track) {
                Ok(track) => {
                    self.player.enqueue(track);
                    self.message = Some("Трек добавлен в очередь".into());
                }
                Err(error) => self.error = Some(format!("{error:#}")),
            }
        }
        cx.notify();
    }

    fn remove_from_playlist(&mut self, track: &Track, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_playlist {
            let service = self.playlists_service.clone();
            let track_id = track.id;
            self.run_job(
                "Убираем трек из плейлиста…",
                move || service.remove_track(id, track_id),
                move |app, _, _| {
                    if app.playback_playlist == Some(id) {
                        app.player.remove(track_id);
                    }
                    app.message =
                        Some("Трек убран из плейлиста. Он доступен в общей библиотеке".into());
                },
                cx,
            );
        }
    }

    fn delete_track(&mut self, track: Track, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let player = self.player.clone();
        let service = self.tracks_service.clone();
        self.modal = None;
        self.run_job(
            "Удаляем трек из хранилища…",
            move || {
                player.remove_and_wait(track.id)?;
                service.delete_track(track.id)?;
                Ok(track.title)
            },
            |app, title, _| {
                app.message = Some(format!("«{title}» удалён из хранилища"));
            },
            cx,
        );
    }

    fn delete_playlist(&mut self, playlist: Playlist, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let service = self.playlists_service.clone();
        let id = playlist.id;
        self.modal = None;
        self.run_job(
            "Удаляем плейлист…",
            move || service.delete_playlist(id),
            move |app, (), _| {
                if app.selected_playlist == Some(id) {
                    app.selected_playlist = None;
                    app.home_open = true;
                    app.settings_open = false;
                }
                if app.playback_playlist == Some(id) {
                    app.playback_playlist = None;
                    app.playback_context = "Очередь воспроизведения".into();
                }
                app.message = Some(format!(
                    "«{}» удалён. Треки остались в библиотеке",
                    playlist.name
                ));
            },
            cx,
        );
    }

    fn close_notifications(&mut self, cx: &mut Context<Self>) {
        self.message = None;
        self.error = None;
        self.error_details_open = false;
        self.player.take_error();
        cx.notify();
    }

    fn sidebar(&self, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let selected = self.selected_playlist;
        let mini_download =
            compact && self.busy.is_some() && !matches!(self.modal, Some(Modal::Import));
        div()
            .w(px(if compact { 176. } else { 224. }))
            .h_full()
            .flex_shrink_0()
            .bg(self.surface(colors.panel))
            .border_r_1()
            .border_color(rgb(colors.border))
            .p(px(if compact { 12. } else { 20. }))
            .flex()
            .flex_col()
            .gap(px(if mini_download { 4. } else { 8. }))
            .children((!mini_download).then(|| {
                div()
                    .id("home-logo")
                    .cursor_pointer()
                    .on_click(cx.listener(|app, _, window, cx| {
                        app.settings_open = false;
                        app.home_open = true;
                        app.modal = None;
                        app.search.update(cx, |search, cx| {
                            search.reset();
                            cx.notify();
                        });
                        app.refresh(cx);
                        window.focus(&app.focus);
                    }))
                    .flex()
                    .items_center()
                    .gap_3()
                    .mb(px(if compact { 12. } else { 28. }))
                    .mt_2()
                    .child(
                        img(self.preferences.theme.logo_path())
                            .size(px(if compact { 44. } else { 52. }))
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .child("Shard"),
                    )
            }))
            .children((!mini_download).then(|| {
                div()
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .mb_2()
                    .child("ВАША МУЗЫКА")
            }))
            .children(mini_download.then(|| {
                self.button("mini-home", "⌂ Главная", false)
                    .py_2()
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.settings_open = false;
                        app.home_open = true;
                        app.refresh(cx);
                    }))
            }))
            .child(
                div()
                    .id("library")
                    .flex_shrink_0()
                    .px_3()
                    .py_3()
                    .rounded_lg()
                    .cursor_pointer()
                    .bg(rgb(if selected.is_none() {
                        colors.active
                    } else {
                        colors.panel
                    }))
                    .text_color(rgb(if selected.is_none() {
                        colors.accent
                    } else {
                        colors.text
                    }))
                    .hover(move |s| s.bg(rgb(colors.button_hover)))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_color(rgb(colors.accent)).child("▤"))
                    .child(div().flex_1().child("Все треки"))
                    .child(div().text_xs().child(self.tracks.len().to_string()))
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.settings_open = false;
                        app.home_open = false;
                        app.selected_playlist = None;
                        app.search.update(cx, |s, cx| {
                            s.reset();
                            cx.notify();
                        });
                        app.refresh(cx);
                    })),
            )
            .child(
                div()
                    .mt(px(if compact { 12. } else { 28. }))
                    .mb_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors.muted))
                            .child("ПЛЕЙЛИСТЫ"),
                    )
                    .child(
                        div()
                            .id("new-playlist")
                            .cursor_pointer()
                            .text_lg()
                            .text_color(rgb(colors.muted))
                            .hover(move |s| s.text_color(rgb(colors.accent)))
                            .child("+")
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.open_modal(Modal::CreatePlaylist, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .id("playlist-scroll")
                    .flex_1()
                    .min_h(px(if mini_download { 48. } else { 0. }))
                    .overflow_y_scroll()
                    .children(self.playlists.iter().map(|playlist| {
                        let id = playlist.id;
                        let delete = playlist.clone();
                        div()
                            .id(("playlist", id as u64))
                            .px_3()
                            .py_3()
                            .mb_1()
                            .rounded_lg()
                            .cursor_pointer()
                            .bg(rgb(if selected == Some(id) {
                                colors.active
                            } else {
                                colors.panel
                            }))
                            .hover(move |s| s.bg(rgb(colors.button_hover)))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .items_center()
                                    .child(self.playlist_artwork(playlist, 24.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(playlist.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(colors.muted))
                                            .child(playlist.track_count.to_string()),
                                    ),
                            )
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.settings_open = false;
                                app.home_open = false;
                                app.selected_playlist = Some(id);
                                app.search.update(cx, |s, cx| {
                                    s.reset();
                                    cx.notify();
                                });
                                app.refresh(cx);
                            }))
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |app, _, window, cx| {
                                    cx.stop_propagation();
                                    app.open_modal(
                                        Modal::DeletePlaylist(delete.clone()),
                                        window,
                                        cx,
                                    );
                                }),
                            )
                    })),
            )
            .children(self.download_card(cx))
            .child(
                self.button("open-settings", "⚙  Настройки", self.settings_open)
                    .py_2()
                    .on_click(cx.listener(|app, _, window, cx| {
                        app.settings_open = true;
                        app.modal = None;
                        app.blur_preview = None;
                        window.focus(&app.focus);
                        cx.notify();
                    })),
            )
            .children((!compact).then(|| {
                div()
                    .pt_4()
                    .border_t_1()
                    .border_color(rgb(colors.border))
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .child("●  Локальная коллекция")
                    .child(div().mt_2().child("Ваша музыка. На вашем устройстве."))
            }))
            .into_any_element()
    }

    fn modal_view(&self, viewport: Size<Pixels>, cx: &mut Context<Self>) -> Option<AnyElement> {
        let colors = self.colors();
        let modal = self.modal.clone()?;
        let (eyebrow, title, description) = match &modal {
            Modal::DeletePlaylist(playlist) => (
                "УПРАВЛЕНИЕ ПЛЕЙЛИСТОМ",
                "Удалить плейлист?",
                format!(
                    "«{}» будет удалён вместе с обложкой и фоном. Все треки останутся в общей библиотеке и других плейлистах.",
                    playlist.name
                ),
            ),
            Modal::PlaylistAppearance(_) => (
                "ОФОРМЛЕНИЕ ПОДБОРКИ",
                "Оформление плейлиста",
                "Выберите обложку — цвет шапки подстроится под неё. Можно задать свой оттенок."
                    .to_owned(),
            ),
            Modal::Import => (
                "ПОПОЛНИТЬ КОЛЛЕКЦИЮ",
                "Добавить музыку",
                "Выберите аудиофайлы или вставьте ссылку на трек или плейлист YouTube.".to_owned(),
            ),
            Modal::CreatePlaylist => (
                "НОВАЯ ПОДБОРКА",
                "Создать плейлист",
                "Дайте название своей следующей музыкальной истории.".to_owned(),
            ),
            Modal::AddToPlaylist(track) => (
                "ДОБАВИТЬ В ПОДБОРКУ",
                "Выберите плейлист",
                format!("Трек: {}", track.title),
            ),
            Modal::PickTracks(_) => (
                "ИЗ ВАШЕЙ БИБЛИОТЕКИ",
                "Добавить треки",
                "Нажмите на трек, чтобы добавить его в этот плейлист.".to_owned(),
            ),
            Modal::TrackActions(track) => (
                "УПРАВЛЕНИЕ ТРЕКОМ",
                "Действия с треком",
                track.title.clone(),
            ),
            Modal::DeleteTrack(track) => (
                "УДАЛЕНИЕ ИЗ ХРАНИЛИЩА",
                "Удалить трек?",
                format!(
                    "«{}» будет удалён из библиотеки, всех плейлистов и папки приложения. Исходный файл на вашем устройстве сохранится.",
                    track.title
                ),
            ),
        };
        let mut card = div()
            .id("modal-card")
            .w(px(560.).min(viewport.width - px(32.)))
            .max_h(px(640.).min(viewport.height - px(32.)))
            .flex_shrink_0()
            .overflow_y_scroll()
            .p_7()
            .rounded_2xl()
            .bg(self.surface(colors.panel))
            .border_1()
            .border_color(rgb(colors.border))
            .shadow_lg()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(rgb(colors.accent))
                            .child(eyebrow),
                    )
                    .children(matches!(modal, Modal::Import).then(|| {
                        self.icon_button("minimize-import", "−", false).on_click(
                            cx.listener(|app, _, window, cx| app.dismiss(&Dismiss, window, cx)),
                        )
                    }))
                    .child(
                        div()
                            .id("close-modal")
                            .cursor_pointer()
                            .text_color(rgb(colors.muted))
                            .child("✕")
                            .on_click(
                                cx.listener(|app, _, window, cx| app.dismiss(&Dismiss, window, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(colors.muted))
                    .child(description),
            );
        match modal {
            Modal::DeletePlaylist(playlist) => {
                card = card
                    .child(
                        self.button("confirm-delete-playlist", "Удалить плейлист", false)
                            .text_color(rgb(0xf1aaa0))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.delete_playlist(playlist.clone(), cx);
                                window.focus(&app.focus);
                            })),
                    )
                    .child(
                        self.button("cancel-delete-playlist", "Отмена", false)
                            .on_click(
                                cx.listener(|app, _, window, cx| app.dismiss(&Dismiss, window, cx)),
                            ),
                    );
            }
            Modal::PlaylistAppearance(id) => {
                if let Some(playlist) = self.playlists.iter().find(|p| p.id == id) {
                    card = card.child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_4()
                            .child(self.playlist_artwork(playlist, 96.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_lg()
                                    .text_ellipsis()
                                    .overflow_hidden()
                                    .child(playlist.name.clone()),
                            ),
                    );
                }
                card = card
                    .child(
                        self.button("choose-cover", "Выбрать обложку…", true)
                            .on_click(cx.listener(move |app, _, _, cx| app.choose_cover(id, cx))),
                    )
                    .child(
                        div().flex().flex_wrap().gap_2().children(
                            [
                                0x314c3b, 0x334b61, 0x4a3a60, 0x643f4c, 0x70523c, 0x234f54,
                                0x3d414c, 0x202126,
                            ]
                            .into_iter()
                            .map(|color| {
                                div()
                                    .id(("header-color", color as u64))
                                    .size_8()
                                    .rounded_lg()
                                    .bg(rgb(color))
                                    .border_2()
                                    .border_color(rgb(
                                        if self
                                            .playlists
                                            .iter()
                                            .find(|p| p.id == id)
                                            .is_some_and(|p| p.header_color == Some(color))
                                        {
                                            colors.accent
                                        } else {
                                            colors.border
                                        },
                                    ))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        app.set_playlist_color(id, Some(color), cx)
                                    }))
                            }),
                        ),
                    )
                    .child(self.input.clone())
                    .child(
                        self.button("apply-header-color", "Применить свой цвет", false)
                            .on_click(
                                cx.listener(|app, _, window, cx| app.submit(&Submit, window, cx)),
                            ),
                    )
                    .child(
                        self.button("auto-header-color", "Подбирать цвет по обложке", false)
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.set_playlist_color(id, None, cx)
                            })),
                    )
                    .child(
                        self.button("remove-cover", "Убрать обложку", false)
                            .on_click(cx.listener(move |app, _, _, cx| {
                                let service = app.appearance_service.clone();
                                app.run_job(
                                    "Убираем обложку…",
                                    move || service.remove_cover(id),
                                    |_, (), _| {},
                                    cx,
                                );
                            })),
                    )
                    .child(self.background_controls(id, cx));
            }
            Modal::Import => {
                card = card
                    .child(
                        self.button("local-file", "↗   Выбрать аудиофайлы", false).on_click(
                            cx.listener(|app, _, window, cx| app.import_local(window, cx)),
                        ),
                    )
                    .child(
                        div()
                            .mt_2()
                            .text_xs()
                            .text_color(rgb(colors.muted))
                            .child("ИЛИ ССЫЛКА НА ТРЕК / ПЛЕЙЛИСТ YOUTUBE"),
                    )
                    .child(self.input.clone())
                    .child(self.button("download", "Скачать и добавить", true).on_click(
                        cx.listener(|app, _, window, cx| app.submit(&Submit, window, cx)),
                    ))
                    .child(div().text_xs().text_color(rgb(colors.muted)).child(
                        "По ссылке на плейлист скачаются все доступные треки. Плейлист сохранится в приложении.",
                    ));
            }
            Modal::CreatePlaylist => {
                card = card.child(self.input.clone()).child(
                    self.button("create", "Создать плейлист", true).on_click(
                        cx.listener(|app, _, window, cx| app.submit(&Submit, window, cx)),
                    ),
                )
            }
            Modal::AddToPlaylist(track) => {
                let id = track.id;
                card = card.child(
                    div()
                        .id("choose-playlist")
                        .min_h_0()
                        .overflow_y_scroll()
                        .children(self.playlists.iter().map(|p| {
                            let playlist_id = p.id;
                            div()
                                .id(("choose", p.id as u64))
                                .p_3()
                                .mb_1()
                                .rounded_lg()
                                .cursor_pointer()
                                .hover(move |s| s.bg(rgb(colors.active)))
                                .child(p.name.clone())
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    app.add_to_playlist(playlist_id, id, cx);
                                    app.modal = None;
                                    window.focus(&app.focus);
                                }))
                        })),
                );
                if self.playlists.is_empty() {
                    card = card.child(
                        div()
                            .text_color(rgb(colors.muted))
                            .child("Пока нет плейлистов. Создайте первый кнопкой ниже."),
                    );
                }
                card = card.child(
                    self.button("create-first", "+  Новый плейлист", false)
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.open_modal(Modal::CreatePlaylist, window, cx)
                        })),
                );
            }
            Modal::PickTracks(playlist) => {
                let current = &self.visible_tracks;
                card = card.child(
                    div()
                        .id("choose-tracks")
                        .min_h_0()
                        .overflow_y_scroll()
                        .children(self.tracks.iter().map(|track| {
                            let id = track.id;
                            let added = current.iter().any(|t| t.id == id);
                            div()
                                .id(("pick", id as u64))
                                .p_3()
                                .mb_1()
                                .rounded_lg()
                                .cursor_pointer()
                                .hover(move |s| s.bg(rgb(colors.active)))
                                .flex()
                                .justify_between()
                                .gap_3()
                                .child(
                                    div()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(track.title.clone()),
                                )
                                .child(div().text_color(rgb(colors.accent)).child(if added {
                                    "✓"
                                } else {
                                    "+"
                                }))
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.add_to_playlist(playlist, id, cx)
                                }))
                        })),
                );
                if self.tracks.is_empty() {
                    card = card.child(
                        div()
                            .text_color(rgb(colors.muted))
                            .child("Сначала добавьте музыку в общую библиотеку."),
                    );
                }
            }
            Modal::TrackActions(track) => {
                let queued = track.clone();
                let add = track.clone();
                let remove = track.clone();
                let delete = track;
                card = card
                    .child(
                        self.button("enqueue-track", "≡  Добавить в очередь", false)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.enqueue_track(&queued, cx);
                                app.modal = None;
                                window.focus(&app.focus);
                            })),
                    )
                    .child(
                        self.button("action-add-playlist", "+  Добавить в плейлист", false)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.open_modal(Modal::AddToPlaylist(add.clone()), window, cx)
                            })),
                    );
                if self.selected_playlist.is_some() {
                    card = card.child(
                        self.button(
                            "remove-from-playlist",
                            "−  Убрать из этого плейлиста",
                            false,
                        )
                        .on_click(cx.listener(
                            move |app, _, window, cx| {
                                app.remove_from_playlist(&remove, cx);
                                app.modal = None;
                                window.focus(&app.focus);
                            },
                        )),
                    );
                }
                card = card.child(
                    self.button("delete-from-storage", "Удалить из хранилища…", false)
                        .text_color(rgb(0xf1aaa0))
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.open_modal(Modal::DeleteTrack(delete.clone()), window, cx)
                        })),
                );
            }
            Modal::DeleteTrack(track) => {
                card = card
                    .child(
                        self.button("confirm-delete-track", "Удалить файл и трек", false)
                            .bg(rgb(0x713e37))
                            .text_color(rgb(0xffe3dd))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.delete_track(track.clone(), cx);
                                window.focus(&app.focus);
                            })),
                    )
                    .child(
                        self.button("cancel-delete-track", "Отмена", false)
                            .on_click(
                                cx.listener(|app, _, window, cx| app.dismiss(&Dismiss, window, cx)),
                            ),
                    );
            }
        }
        if let Some(busy) = &self.busy {
            card = card.child(
                div()
                    .text_sm()
                    .text_color(rgb(colors.accent))
                    .child(busy.clone()),
            );
        }
        if let Some(cancel) = &self.download_cancel {
            let cancel = cancel.clone();
            card = card.child(
                self.button("cancel-download", "Остановить загрузку", false)
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cancel.store(true, Ordering::Relaxed);
                        cx.notify();
                    })),
            );
        }
        card = card.children(self.notifications(cx));
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(rgba(0x060a08cc))
                .flex()
                .items_center()
                .justify_center()
                .child(card)
                .into_any_element(),
        )
    }
}

impl MusicApp {
    fn button(&self, id: &'static str, label: &str, primary: bool) -> Stateful<Div> {
        let colors = self.colors();
        div()
            .id(id)
            .px_4()
            .py_3()
            .flex_shrink_0()
            .rounded_lg()
            .cursor_pointer()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .bg(rgb(if primary {
                colors.accent
            } else {
                colors.button
            }))
            .text_color(rgb(if primary { colors.bg } else { colors.text }))
            .hover(move |s| {
                s.bg(rgb(if primary {
                    colors.accent
                } else {
                    colors.button_hover
                }))
            })
            .child(label.to_owned())
    }
}

fn format_duration(ms: Option<i64>) -> String {
    match ms {
        Some(ms) => format!("{}:{:02}", ms.max(0) / 60_000, ms.max(0) / 1000 % 60),
        None => "—".into(),
    }
}
