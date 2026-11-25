use shard::{i18n::tr, localized_format};
#[path = "ui/artwork.rs"]
mod artwork;
#[path = "ui/background_animation.rs"]
mod background_animation;
#[path = "ui/components.rs"]
mod components;
#[path = "ui/cursor.rs"]
mod cursor;
#[path = "ui/devices.rs"]
mod devices;
#[path = "ui/downloads.rs"]
mod downloads;
use downloads::DownloadResult;
#[path = "ui/home.rs"]
mod home;
#[path = "ui/ignored.rs"]
mod ignored;
#[path = "ui/image_cache.rs"]
mod image_cache;
#[path = "ui/now_playing.rs"]
mod now_playing;
#[path = "ui/progress.rs"]
mod progress;
#[path = "ui/settings.rs"]
mod settings;
#[path = "ui/sidebar.rs"]
mod sidebar;
#[path = "ui/storage.rs"]
mod storage;
#[path = "ui/synchronization.rs"]
mod synchronization;
#[path = "ui/updates.rs"]
mod updates;

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
        VolumeDown,
        FullscreenTrack
    ]
);
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("f11", FullscreenTrack, Some("MusicApp && !TextInput")),
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
    ConfirmUpdate,
    SyncHelp,
    SyncOptions(shard::sync::SyncSelection),
    SyncOptionsHelp(shard::sync::SyncSelection),
    SyncDuplicates,
    PlaylistActions(Playlist),
    EditTrack(Track),
    PlaylistAppearance(i64),
    Import,
    Downloads,
    MoveStorage(PathBuf),
    ClearStorage,
    MergePlaylist(Playlist),
    CreatePlaylist,
    AddToPlaylist(Track),
    PickTracks(i64),
    TrackActions(Track),
    DeleteTrack(Track),
    DeleteIgnoredTracks(Vec<Track>),
    DeletePlaylist(Playlist),
}

pub(crate) struct UiTheme(pub Theme);
impl Global for UiTheme {}

pub struct MusicApp {
    images: Entity<image_cache::ArtworkCache>,
    progress: Entity<progress::PlaybackProgress>,
    cursor_companion: Entity<cursor::CursorCompanion>,
    library_body: Option<Entity<components::LibraryBody>>,
    now_playing_open: bool,
    restore_fullscreen: bool,
    library_renders: u64,
    image_cache_dirty: bool,
    library_loading: bool,
    thumbnail_cancel: Option<Arc<AtomicBool>>,
    thumbnail_queue: std::collections::VecDeque<PathBuf>,
    thumbnail_pending: std::collections::HashSet<PathBuf>,
    preview_paths: std::collections::HashSet<PathBuf>,
    image_invalidations: Vec<PathBuf>,
    favorites_service: shard::favorites::FavoritesService,
    favorite_ids: std::collections::HashSet<i64>,
    favorite_pending: std::collections::HashSet<i64>,
    ignored_service: shard::ignored::IgnoredTracksService,
    ignored_ids: std::collections::HashSet<i64>,
    sync_service: Option<shard::sync::SyncService>,
    sync_status: shard::sync::SyncStatus,
    sync_input: Entity<TextInput>,
    sync_qr: Option<PathBuf>,
    sync_job_generation: u64,
    sync_duplicate_requests: Option<mpsc::Receiver<synchronization::DuplicateRequest>>,
    sync_duplicate_prompt: Option<synchronization::DuplicatePrompt>,
    settings_open: bool,
    updates: updates::UpdateUi,
    storage_moving: bool,
    storage_clearing: bool,
    storage_ready: bool,
    settings_category: settings::SettingsCategory,
    customization_category: settings::CustomizationCategory,
    pet_picker: Option<Entity<progress::PetPicker>>,
    playlist_name: Entity<TextInput>,
    playlist_name_task: Option<Task<()>>,
    playlist_name_pending: std::collections::HashMap<i64, String>,
    header_color_draft: Option<(i64, Hsla)>,
    background_service: BackgroundService,
    backgrounds: Vec<Background>,
    background_animation: background_animation::BackgroundAnimation,
    blur_preview: Option<u8>,
    collection_list: ListState,
    collection_tracks: Rc<Vec<Track>>,
    collection_dirty: bool,
    collection_items: Vec<i64>,
    collection_query: String,
    scroll_context: Option<i64>,
    home_open: bool,
    home_playlists_expanded: bool,
    home_activity_expanded: bool,
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
    artwork_cancel: Option<Arc<AtomicBool>>,
    playlists: Vec<Playlist>,
    visible_tracks: Vec<Track>,
    visible_duration_ms: i64,
    selected_playlist: Option<i64>,
    modal: Option<Modal>,
    input: Entity<TextInput>,
    artist_input: Entity<TextInput>,
    search: Entity<TextInput>,
    focus: FocusHandle,
    busy: Option<String>,
    message: Option<String>,
    error: Option<String>,
    player: Player,
    pending_downloads: std::collections::VecDeque<String>,
    active_download: Option<String>,
    download_history: Vec<DownloadResult>,
    download_progress: Option<mpsc::Receiver<YoutubeProgress>>,
    download_cancel: Option<Arc<AtomicBool>>,
    preferences: Preferences,
    preferences_writer: PreferencesWriter,
    data_directory: PathBuf,
    error_details_open: bool,
    playback_context: String,
    playback_playlist: Option<i64>,
    playback_query: String,
    slider_bounds: [Option<Bounds<Pixels>>; 6],
    dragging_slider: Option<components::SliderKind>,
    last_volume: f32,
}

impl MusicApp {
    fn set_language(&mut self, language: shard::i18n::Language, cx: &mut Context<Self>) {
        self.preferences.language = language;
        shard::i18n::set_language(language);
        self.search.update(cx, |input, cx| {
            input.set_placeholder(tr("Поиск по названию или исполнителю"));
            cx.notify();
        });
        if let Some(id) = self.playback_playlist {
            if let Some(playlist) = self.playlists.iter().find(|p| p.id == id) {
                self.playback_context = playlist.name.clone();
            }
        } else {
            self.playback_context = tr("Ваша библиотека").into();
        }
        self.message = None;
        self.error = None;
        self.collection_dirty = true;
        if let Some(playlist) = self
            .playlists
            .iter_mut()
            .find(|p| p.id == shard::favorites::FAVORITES_ID)
        {
            playlist.name = tr("Избранное").into();
        }
        self.save_preferences();
        #[cfg(windows)]
        crate::desktop::refresh_language(cx);
        cx.notify();
    }
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
        self.artist_input.update(cx, |_, cx| cx.notify());
        self.playlist_name.update(cx, |_, cx| cx.notify());
        self.save_preferences();
        cx.notify();
    }

    pub fn new(db: Database, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| TextInput::new(tr("Поиск по названию или исполнителю"), cx));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        let focus = cx.focus_handle();
        let data_directory = db.root().to_path_buf();
        let preferences = Preferences::load(&data_directory);
        shard::i18n::set_language(preferences.language);
        search.update(cx, |search, _| {
            search.set_placeholder(tr("Поиск по названию или исполнителю"))
        });
        let player = Player::new(preferences.volume, preferences.shuffle, Some(db.clone()));
        player.set_repeat(preferences.repeat);
        player.set_output_device(preferences.audio_output.clone());
        player.set_monitoring(preferences.audio_monitoring);
        player.set_obs_stream(preferences.obs_stream);
        cx.set_global(UiTheme(preferences.theme));
        window.focus(&focus);
        let mut app = Self {
            images: image_cache::ArtworkCache::new(cx),
            progress: cx.new(|cx| progress::PlaybackProgress::new(player.clone(), window, cx)),
            cursor_companion: cx.new(|cx| cursor::CursorCompanion::new(window, cx)),
            library_body: None,
            now_playing_open: false,
            restore_fullscreen: false,
            library_renders: 0,
            image_cache_dirty: false,
            library_loading: false,
            thumbnail_cancel: None,
            thumbnail_queue: Default::default(),
            thumbnail_pending: Default::default(),
            preview_paths: Default::default(),
            image_invalidations: vec![],
            favorites_service: shard::favorites::FavoritesService::new(db.clone()),
            favorite_ids: Default::default(),
            favorite_pending: Default::default(),
            ignored_service: shard::ignored::IgnoredTracksService::new(db.clone()),
            ignored_ids: Default::default(),
            sync_service: None,
            sync_status: Default::default(),
            sync_input: cx.new(|cx| TextInput::new(tr("Вставьте код другого устройства"), cx)),
            sync_qr: None,
            sync_job_generation: 0,
            sync_duplicate_requests: None,
            sync_duplicate_prompt: None,
            settings_open: false,
            updates: Default::default(),
            storage_moving: false,
            storage_clearing: false,
            storage_ready: false,
            settings_category: settings::SettingsCategory::Customization,
            customization_category: settings::CustomizationCategory::Themes,
            pet_picker: None,
            playlist_name: cx.new(|cx| TextInput::new(tr("Название плейлиста"), cx)),
            playlist_name_task: None,
            playlist_name_pending: Default::default(),
            header_color_draft: None,
            background_service: BackgroundService::new(db.clone()),
            backgrounds: vec![],
            background_animation: Default::default(),
            blur_preview: None,
            collection_list: ListState::new(1, ListAlignment::Top, px(256.)),
            collection_tracks: Rc::new(vec![]),
            collection_dirty: true,
            collection_items: vec![],
            collection_query: String::new(),
            scroll_context: None,
            home_open: true,
            home_playlists_expanded: false,
            home_activity_expanded: false,
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
            artwork_cancel: None,
            playlists: vec![],
            visible_tracks: vec![],
            visible_duration_ms: 0,
            selected_playlist: None,
            modal: None,
            input: cx.new(|cx| TextInput::new("", cx)),
            artist_input: cx.new(|cx| TextInput::new(tr("Исполнитель"), cx)),
            search,
            focus,
            busy: None,
            message: None,
            error: None,
            player,
            pending_downloads: Default::default(),
            active_download: None,
            download_history: vec![],
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
            playback_context: tr("Ваша библиотека").into(),
            playback_playlist: None,
            playback_query: String::new(),
            slider_bounds: [None; 6],
            dragging_slider: None,
        };
        let name_focus = app.playlist_name.focus_handle(cx);
        cx.on_blur(&name_focus, window, |app, _, cx| {
            app.commit_playlist_name(cx)
        })
        .detach();
        cx.observe_window_activation(window, |_, _, cx| cx.notify())
            .detach();
        app.save_preferences();
        if app.preferences.sync_enabled {
            app.start_sync(cx);
        }
        app.refresh(cx);
        app.schedule_update_checks(cx);
        cx.on_release(|app, cx| {
            app.updates.cancel.store(true, Ordering::Relaxed);
            drop(app.background_animation.suspend(cx));
            if let Some(cancel) = &app.thumbnail_cancel {
                cancel.store(true, Ordering::Relaxed);
            }
            if let Some(path) = app.sync_qr.take() {
                let _ = std::fs::remove_file(path);
            }
            if let Some(service) = &app.sync_service {
                service.stop();
            }
            app.player.flush_and_wait();
            app.preferences_writer.flush_and_wait();
        })
        .detach();
        cx.on_app_quit(|app, cx| {
            app.updates.cancel.store(true, Ordering::Relaxed);
            drop(app.background_animation.suspend(cx));
            if let Some(cancel) = &app.thumbnail_cancel {
                cancel.store(true, Ordering::Relaxed);
            }
            if let Some(service) = &app.sync_service {
                service.stop();
            }
            app.commit_playlist_name(cx);
            if let Some(cancel) = &app.download_cancel {
                cancel.store(true, Ordering::Relaxed);
            }
            if let Some(cancel) = &app.artwork_cancel {
                cancel.store(true, Ordering::Relaxed);
            }
            app.player.stop();
            app.player.flush_and_wait();
            app.preferences_writer.flush_and_wait();
            let name_task = app.playlist_name_task.take();
            async move {
                if let Some(task) = name_task {
                    task.await;
                }
            }
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
                        let progress = app.player.progress();
                        let snapshot = (progress.0, progress.1);
                        let mut changed = last_player != Some(snapshot);
                        changed |= app.poll_update_progress();
                        if let Some(request) = app
                            .sync_duplicate_requests
                            .as_ref()
                            .and_then(|receiver| receiver.try_recv().ok())
                        {
                            app.sync_duplicate_prompt =
                                Some(synchronization::DuplicatePrompt::new(request));
                            app.open_modal(Modal::SyncDuplicates, window, cx);
                            changed = true;
                        }
                        if let Some(service) = &app.sync_service {
                            let status = service.status();
                            if status != app.sync_status {
                                let refresh = status.revision != app.sync_status.revision;
                                app.sync_status = status;
                                changed = true;
                                if refresh {
                                    app.refresh(cx);
                                }
                            }
                        }
                        last_player = Some(snapshot);
                        if (window.is_window_active() || app.preferences.refresh_inactive)
                            && app.home_open
                            && snapshot.0.is_some()
                            && !snapshot.1
                            && app.statistics_updated.elapsed() >= Duration::from_secs(5)
                        {
                            app.refresh_statistics(cx);
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
                            let imported = app
                                .job_progress
                                .as_ref()
                                .is_some_and(|previous| progress.completed > previous.completed);
                            app.job_progress = Some(progress.clone());
                            changed = true;
                            if imported {
                                app.refresh(cx);
                            }
                        }
                        if changed
                            && (window.is_window_active() || app.preferences.refresh_inactive)
                        {
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
        if self.storage_clearing || self.storage_moving {
            return;
        }
        if self.home_open {
            self.refresh_statistics(cx);
        }
        self.library_generation += 1;
        if self.library_loading {
            return;
        }
        self.library_loading = true;
        let generation = self.library_generation;
        let selected = self.selected_playlist;
        if self.loaded_playlist != selected {
            self.visible_tracks.clear();
            self.visible_duration_ms = 0;
            self.collection_dirty = true;
        }
        let tracks = self.tracks_service.clone();
        let playlists = self.playlists_service.clone();
        let backgrounds = self.background_service.clone();
        let favorites = self.favorites_service.clone();
        let ignored = self.ignored_service.clone();
        let data_root = self.data_directory.clone();
        let task = cx.background_executor().spawn(async move {
            let tracks = tracks.list_tracks()?;
            let visible = match selected {
                Some(id) => playlists.list_playlist_tracks(id)?,
                None => tracks.clone(),
            };
            let mut lists = playlists.list_playlists()?;
            lists.insert(0, favorites.playlist()?);
            let previews = shard::artwork::PreviewPlan::inspect(
                lists
                    .iter()
                    .filter_map(|playlist| {
                        playlist
                            .cover_path
                            .as_ref()
                            .map(|path| data_root.join(path))
                    })
                    .chain(
                        visible
                            .iter()
                            .map(|track| shard::artwork::track_path(&data_root, track)),
                    )
                    .chain(
                        tracks
                            .iter()
                            .map(|track| shard::artwork::track_path(&data_root, track)),
                    ),
            );
            Ok::<_, anyhow::Error>((
                tracks,
                lists,
                visible,
                backgrounds.list()?,
                favorites.ids()?,
                ignored.ids()?,
                previews,
            ))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                // A slower previous request must not overwrite a newer navigation or import.
                app.library_loading = false;
                if app.library_generation != generation {
                    app.refresh(cx);
                    return;
                }
                match result {
                    Ok((
                        tracks,
                        playlists,
                        visible,
                        backgrounds,
                        favorite_ids,
                        ignored_ids,
                        previews,
                    )) => {
                        app.favorite_ids = favorite_ids;
                        app.ignored_ids = ignored_ids;
                        app.player.set_ignored(app.ignored_ids.clone());
                        app.preview_paths = previews.ready;
                        app.track_index = tracks
                            .iter()
                            .enumerate()
                            .map(|(index, track)| (track.id, index))
                            .collect();
                        app.player.set_library(
                            tracks
                                .iter()
                                .filter_map(|track| app.playback_track(track).ok())
                                .collect(),
                            app.preferences.autoplay,
                        );
                        app.tracks = tracks;
                        app.playlists = playlists;
                        app.visible_duration_ms =
                            visible.iter().filter_map(|t| t.duration_ms).sum();
                        app.visible_tracks = visible;
                        app.backgrounds = backgrounds;
                        if app.busy.is_none() {
                            app.background_animation.resume();
                        }
                        app.loaded_playlist = selected;
                        app.collection_dirty = true;
                        app.prepare_previews(previews.pending, cx);
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
        self.error = None;
        self.error_details_open = false;
        self.message = None;
        let task = cx.background_executor().spawn(async move { job() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                app.busy = None;
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

    fn commit_playlist_name(&mut self, cx: &mut Context<Self>) {
        let Some(Modal::PlaylistAppearance(id)) = self.modal else {
            return;
        };
        let Some(playlist) = self.playlists.iter().find(|p| p.id == id) else {
            return;
        };
        let name = self.playlist_name.read(cx).value().trim().to_owned();
        if name.is_empty() {
            let previous = self
                .playlist_name_pending
                .get(&id)
                .cloned()
                .unwrap_or_else(|| playlist.name.clone());
            self.playlist_name.update(cx, |input, cx| {
                input.set_value(&previous);
                cx.notify();
            });
            return;
        }
        let current_name = self
            .playlist_name_pending
            .get(&id)
            .unwrap_or(&playlist.name);
        if &name == current_name {
            return;
        }
        self.playlist_name_pending.insert(id, name.clone());
        let previous = self.playlist_name_task.take();
        let service = self.playlists_service.clone();
        self.playlist_name_task = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let saved_name = name.clone();
            let task = cx
                .background_executor()
                .spawn(async move { service.rename_playlist(id, &saved_name) });
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                if app.playlist_name_pending.get(&id) != Some(&name) {
                    return;
                }
                app.playlist_name_pending.remove(&id);
                match result {
                    Ok(()) => {
                        if let Some(playlist) = app.playlists.iter_mut().find(|p| p.id == id) {
                            playlist.name = name.clone();
                        }
                        if app.playback_playlist == Some(id) {
                            app.playback_context = name;
                        }
                        app.collection_dirty = true;
                    }
                    Err(error) => app.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        }));
    }

    fn open_modal(&mut self, modal: Modal, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_playlist_name(cx);
        self.header_color_draft = None;
        let has_input = matches!(
            modal,
            Modal::Import | Modal::CreatePlaylist | Modal::EditTrack(_)
        ) || matches!(&modal, Modal::PlaylistAppearance(id) if self.playlists.iter().find(|p| p.id == *id).is_some_and(|p| p.header_color.is_some()));
        let placeholder = match modal {
            Modal::Import => "https://…",
            Modal::EditTrack(_) => tr("Название трека"),
            Modal::CreatePlaylist => tr("Например, Вечерний маршрут"),
            Modal::PlaylistAppearance(_) => tr("Свой цвет: #34465E"),
            _ => "",
        };
        self.input = cx.new(|cx| TextInput::new(placeholder, cx));
        match &modal {
            Modal::PlaylistAppearance(id) => {
                if let Some(playlist) = self.playlists.iter().find(|p| p.id == *id) {
                    let name = self.playlist_name_pending.get(id).unwrap_or(&playlist.name);
                    self.playlist_name
                        .update(cx, |input, _| input.set_value(name));
                }
                if let Some(color) = self
                    .playlists
                    .iter()
                    .find(|p| p.id == *id)
                    .and_then(|p| p.header_color)
                {
                    self.input
                        .update(cx, |input, _| input.set_value(&format!("#{color:06X}")));
                }
            }
            Modal::EditTrack(track) => {
                self.input
                    .update(cx, |input, _| input.set_value(&track.title));
                self.artist_input = cx.new(|cx| TextInput::new(tr("Исполнитель"), cx));
                self.artist_input
                    .update(cx, |input, _| input.set_value(&track.artist));
            }
            _ => {}
        }
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
        if self.now_playing_open {
            self.toggle_now_playing(window, cx);
            return;
        }
        if let Some(Modal::SyncOptionsHelp(selection)) = self.modal.clone() {
            self.open_modal(Modal::SyncOptions(selection), window, cx);
            return;
        }
        if matches!(self.modal, Some(Modal::SyncDuplicates)) {
            self.cancel_sync_duplicates();
        }
        self.commit_playlist_name(cx);
        if self.modal.take().is_none() && self.preferences.queue_visible {
            self.toggle_queue(cx);
        }
        window.focus(&self.focus);
        cx.notify();
    }

    fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        match self.modal.clone() {
            Some(Modal::EditTrack(track)) => {
                let service = self.tracks_service.clone();
                let title = self.input.read(cx).value();
                let artist = self.artist_input.read(cx).value();
                self.run_job(
                    tr("Сохраняем…"),
                    move || service.update_metadata(track.id, &title, &artist),
                    |app, _, cx| {
                        app.modal = None;
                        app.refresh_statistics(cx);
                    },
                    cx,
                );
                window.focus(&self.focus);
            }
            Some(Modal::PlaylistAppearance(id)) => {
                if self.playlist_name.focus_handle(cx).is_focused(window) {
                    self.commit_playlist_name(cx);
                    window.focus(&self.focus);
                    return;
                }
                if !self
                    .playlists
                    .iter()
                    .find(|p| p.id == id)
                    .is_some_and(|p| p.header_color.is_some())
                {
                    return;
                }
                match shard::appearance::parse_color(&self.input.read(cx).value()) {
                    Ok(color) => self.set_playlist_color(id, Some(color), cx),
                    Err(error) => {
                        self.error = Some(format!("{error:#}"));
                        cx.notify();
                    }
                }
            }
            Some(Modal::Import) => {
                let input = self.input.read(cx).value();
                if input.trim().is_empty() {
                    return;
                }
                let url = match shard::services::validate_music_url(&input) {
                    Ok(url) => url.to_string(),
                    Err(error) => {
                        self.error = Some(format!("{error:#}"));
                        cx.notify();
                        return;
                    }
                };
                if self.active_download.as_ref() == Some(&url)
                    || self.pending_downloads.contains(&url)
                {
                    self.message = Some(localized_format!(
                        "Эта ссылка уже в очереди загрузок",
                        "This link is already queued"
                    ));
                } else {
                    self.pending_downloads.push_back(url);
                    self.start_next_download(cx);
                    self.input.update(cx, |input, cx| {
                        input.reset();
                        cx.notify();
                    });
                    self.message = Some(localized_format!(
                        "Добавлено в очередь загрузок",
                        "Added to download queue"
                    ));
                }
                cx.notify();
                window.focus(&self.input.focus_handle(cx));
            }
            Some(Modal::CreatePlaylist) => {
                let service = self.playlists_service.clone();
                let name = self.input.read(cx).value();
                self.run_job(
                    tr("Создаём плейлист…"),
                    move || service.create_playlist(&name),
                    |app, id, _| {
                        app.settings_open = false;
                        app.home_open = false;
                        app.selected_playlist = Some(id);
                        app.modal = None;
                        app.message =
                            Some(tr("Плейлист создан. Добавьте в него треки из библиотеки").into());
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
            tr("Добавляем локальные файлы…"),
            move || {
                // GPUI's worker pool uses MTA; Windows file dialogs require their own STA thread.
                let paths = std::thread::Builder::new()
                    .name("shard-file-picker".into())
                    .spawn(|| {
                        rfd::FileDialog::new()
                            .set_title(tr("Добавить музыку в Shard"))
                            .add_filter(tr("Аудиофайлы"), AUDIO_EXTENSIONS)
                            .pick_files()
                            .unwrap_or_default()
                    })?
                    .join()
                    .map_err(|_| anyhow::anyhow!(tr("Не удалось открыть окно выбора файла")))?;
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
                    app.message = Some(localized_format!(
                        "Добавлено треков: {count}",
                        "Tracks added: {count}"
                    ));
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
            tr("Добавляем трек в плейлист…"),
            move || service.add_track(playlist, track),
            |app, added, _| {
                app.message = Some(
                    if added {
                        tr("Трек добавлен в плейлист")
                    } else {
                        tr("Этот трек уже есть в плейлисте")
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
            .unwrap_or(tr("Все треки").into())
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
            let context = if self.settings_open {
                vec![track.clone()]
            } else {
                self.filtered_tracks(cx)
            };
            let result: Result<Vec<_>> = context
                .iter()
                .map(|track| self.playback_track(track))
                .collect();
            match result {
                Ok(tracks) => {
                    self.playback_context = if self.settings_open {
                        tr("Игнорирование").into()
                    } else {
                        self.current_title()
                    };
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
        } else if let Some(track) = self
            .filtered_tracks(cx)
            .into_iter()
            .find(|track| !self.ignored_ids.contains(&track.id))
        {
            self.play(track, cx);
        }
        cx.notify();
    }

    fn play_collection(&mut self, cx: &mut Context<Self>) {
        if self.player.state().track_id.is_some()
            && self.playback_playlist == self.selected_playlist
        {
            self.player.toggle();
        } else if let Some(track) = self
            .filtered_tracks(cx)
            .into_iter()
            .find(|track| !self.ignored_ids.contains(&track.id))
        {
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

    fn playback_mode(&self) -> shard::queue::PlaybackMode {
        shard::queue::PlaybackMode::from_flags(self.preferences.shuffle, self.preferences.autoplay)
    }

    fn cycle_playback_mode(&mut self, cx: &mut Context<Self>) {
        let (shuffle, autoplay) = self.playback_mode().next().flags();
        self.preferences.shuffle = shuffle;
        self.preferences.autoplay = autoplay;
        self.player.set_shuffle(shuffle);
        self.player.set_library(
            self.tracks
                .iter()
                .filter_map(|track| self.playback_track(track).ok())
                .collect(),
            autoplay,
        );
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
            tr("Готовим обложку…"),
            move || {
                let path = std::thread::Builder::new()
                    .name("shard-cover-picker".into())
                    .spawn(|| {
                        rfd::FileDialog::new()
                            .set_title(tr("Обложка плейлиста"))
                            .add_filter(tr("Изображения"), &["png", "jpg", "jpeg", "webp"])
                            .pick_file()
                    })?
                    .join()
                    .map_err(|_| anyhow::anyhow!(tr("Не удалось открыть выбор изображения")))?;
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
        if self.busy.is_some() {
            return;
        }
        if let Some(color) = color {
            if self.header_color_draft.is_none_or(|(draft_id, draft)| {
                draft_id != id || Self::header_color_value(draft) != color
            }) {
                self.header_color_draft = Some((id, rgb(color).into()));
            }
            self.input.update(cx, |input, cx| {
                input.set_value(&format!("#{color:06X}"));
                cx.notify();
            });
        } else {
            self.header_color_draft = None;
            self.input.update(cx, |input, cx| {
                input.reset();
                cx.notify();
            });
        }
        let service = self.appearance_service.clone();
        self.run_job(
            tr("Сохраняем цвет плейлиста…"),
            move || service.set_header_color(id, color),
            |_, (), _| {},
            cx,
        );
    }

    fn enqueue_track(&mut self, track: &Track, cx: &mut Context<Self>) {
        if self.ignored_ids.contains(&track.id) {
            self.message = Some(
                tr("Этот трек игнорируется. Запустите его напрямую или уберите из игнора.").into(),
            );
        } else if self.player.state().queue.contains(track.id) {
            self.message = Some(tr("Этот трек уже есть в очереди").into());
        } else {
            match self.playback_track(track) {
                Ok(track) => {
                    self.player.enqueue(track);
                    self.message = Some(tr("Трек добавлен в очередь").into());
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
                tr("Убираем трек из плейлиста…"),
                move || service.remove_track(id, track_id),
                move |app, _, _| {
                    if app.playback_playlist == Some(id) {
                        app.player.remove(track_id);
                    }
                    app.message =
                        Some(tr("Трек убран из плейлиста. Он доступен в общей библиотеке").into());
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
            tr("Удаляем трек из хранилища…"),
            move || {
                player.remove_and_wait(track.id)?;
                service.delete_track(track.id)?;
                Ok(track.title)
            },
            |app, title, _| {
                app.message = Some(localized_format!(
                    "«{title}» удалён из хранилища",
                    "“{title}” deleted from storage"
                ));
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
        let video = self.background_animation.suspend(cx);
        self.run_job(
            tr("Удаляем плейлист…"),
            move || {
                if let Some(video) = video {
                    video.stop();
                }
                service.delete_playlist(id)
            },
            move |app, (), _| {
                if app.selected_playlist == Some(id) {
                    app.selected_playlist = None;
                    app.home_open = true;
                    app.settings_open = false;
                }
                if app.playback_playlist == Some(id) {
                    app.playback_playlist = None;
                    app.playback_context = tr("Очередь воспроизведения").into();
                }
                app.message = Some(localized_format!(
                    "«{}» удалён. Треки остались в библиотеке",
                    "“{}” deleted. Tracks remain in your library",
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

    fn modal_view(&self, viewport: Size<Pixels>, cx: &mut Context<Self>) -> Option<AnyElement> {
        let colors = self.colors();
        let modal = self.modal.clone()?;
        let (eyebrow, title, description) = match &modal {
            Modal::ConfirmUpdate => (
                "SHARD",
                tr("Обновить приложение?"),
                localized_format!(
                    "Будет скачана и установлена версия {}. Shard закроется и откроется снова после обновления. Музыка, плейлисты и настройки сохранятся.",
                    "Version {} will be downloaded and installed. Shard will close and reopen after the update. Your music, playlists and settings will be kept.",
                    self.updates
                        .available
                        .as_ref()
                        .map(|u| u.version.as_str())
                        .unwrap_or("")
                ),
            ),
            Modal::DeleteIgnoredTracks(tracks) => (
                tr("УДАЛЕНИЕ ИЗ ХРАНИЛИЩА"),
                tr("Удалить все игнорируемые треки?"),
                localized_format!(
                    "Из хранилища, библиотеки и всех плейлистов будут удалены игнорируемые треки: {}. Исходные файлы на устройстве сохранятся.",
                    "Ignored tracks will be deleted from storage, your library and all playlists: {}. Original files on your device will be kept.",
                    tracks.len()
                ),
            ),
            Modal::ClearStorage => (
                "SHARD",
                tr("Очистить хранилище?"),
                localized_format!(
                    "Будут безвозвратно удалены все {} треков из папки {} и обложки этих треков. Все плейлисты станут пустыми. Настройки, оформление и статистика сохранятся. Исходные файлы, из которых вы импортировали музыку, не затрагиваются.",
                    "All {} tracks in {} and their track covers will be permanently deleted. All playlists become empty. Settings, appearance and listening totals are kept. Original files used for imports are untouched.",
                    self.tracks.len(),
                    self.data_directory.display()
                ),
            ),
            Modal::SyncOptions(selection) => (
                tr("ПЕРЕДАЧА МУЗЫКИ"),
                tr("Настройки синхронизации"),
                self.sync_selection_title(*selection),
            ),
            Modal::SyncOptionsHelp(_) => (
                tr("ПОМОЩЬ"),
                tr("Как обрабатывать дубликаты"),
                tr("Выберите, что делать с треками, которые уже есть на другом устройстве.").into(),
            ),
            Modal::SyncDuplicates => (
                tr("ПЕРЕДАЧА МУЗЫКИ"),
                tr("Найдены дубликаты"),
                tr("Выберите: использовать имеющийся трек или передать ещё одну копию.").into(),
            ),
            Modal::SyncHelp => (
                tr("ПОМОЩЬ"),
                tr("Как работает синхронизация"),
                tr("Передавайте музыку между устройствами в одной локальной сети.").to_owned(),
            ),
            Modal::MoveStorage(destination) => (
                "SHARD",
                tr("Изменить хранилище"),
                localized_format!(
                    "Перенести библиотеку в {}? Треки, обложки, плейлисты и настройки будут скопированы, затем SHARD перезапустится. Исходная папка сохранится как резервная копия.",
                    "Move the library to {}? Tracks, artwork, playlists and settings will be copied, then SHARD will restart. The original folder stays as a backup.",
                    destination.display()
                ),
            ),
            Modal::Downloads => (
                "SHARD",
                tr("Загрузки"),
                localized_format!(
                    "Альбомы и треки скачиваются по очереди. Можно продолжать пользоваться библиотекой.",
                    "Albums and tracks download in order. Your library stays available."
                ),
            ),
            Modal::MergePlaylist(playlist) => (
                "SHARD",
                tr("Объединить плейлисты"),
                localized_format!(
                    "Выберите пару для «{}». Создадим новый плейлист без повторов; исходные подборки сохранятся.",
                    "Choose a partner for “{}”. A new playlist will contain unique tracks; both originals are kept.",
                    playlist.name
                ),
            ),
            Modal::PlaylistActions(playlist) => (
                tr("УПРАВЛЕНИЕ ПЛЕЙЛИСТОМ"),
                tr("Действия с плейлистом"),
                playlist.name.clone(),
            ),
            Modal::EditTrack(_) => (
                tr("УПРАВЛЕНИЕ ТРЕКОМ"),
                tr("Изменить трек"),
                tr("Измените название и исполнителя в библиотеке.").to_owned(),
            ),
            Modal::DeletePlaylist(playlist) => (
                tr("УПРАВЛЕНИЕ ПЛЕЙЛИСТОМ"),
                tr("Удалить плейлист?"),
                localized_format!(
                    "«{}» будет удалён вместе с обложкой и фоном. Все треки останутся в общей библиотеке и других плейлистах.",
                    "“{}” and its cover and background will be deleted. All tracks remain in your library and other playlists.",
                    playlist.name
                ),
            ),
            Modal::PlaylistAppearance(_) => (
                tr("ОФОРМЛЕНИЕ ПОДБОРКИ"),
                tr("Редактировать плейлист"),
                tr("Название, обложка, цвет шапки и фон плейлиста.").to_owned(),
            ),
            Modal::Import => (
                tr("ПОПОЛНИТЬ КОЛЛЕКЦИЮ"),
                tr("Добавить музыку"),
                tr("YouTube, Spotify или Яндекс Музыка: трек, альбом или плейлист").to_owned(),
            ),
            Modal::CreatePlaylist => (
                tr("НОВАЯ ПОДБОРКА"),
                tr("Создать плейлист"),
                tr("Дайте название своей следующей музыкальной истории.").to_owned(),
            ),
            Modal::AddToPlaylist(track) => (
                tr("ДОБАВИТЬ В ПОДБОРКУ"),
                tr("Выберите плейлист"),
                localized_format!("Трек: {}", "Track: {}", track.title),
            ),
            Modal::PickTracks(_) => (
                tr("ИЗ ВАШЕЙ БИБЛИОТЕКИ"),
                tr("Добавить треки"),
                tr("Нажмите на трек, чтобы добавить его в этот плейлист.").to_owned(),
            ),
            Modal::TrackActions(track) => (
                tr("УПРАВЛЕНИЕ ТРЕКОМ"),
                tr("Действия с треком"),
                track.title.clone(),
            ),
            Modal::DeleteTrack(track) => (
                tr("УДАЛЕНИЕ ИЗ ХРАНИЛИЩА"),
                tr("Удалить трек?"),
                localized_format!(
                    "«{}» будет удалён из библиотеки, всех плейлистов и папки приложения. Исходный файл на вашем устройстве сохранится.",
                    "“{}” will be deleted from your library, all playlists and app storage. The original file on your device will be kept.",
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
                    .children(match &modal {
                        Modal::SyncOptions(selection) => {
                            let selection = *selection;
                            Some(
                                div()
                                    .id("sync-options-help")
                                    .size_7()
                                    .mr_2()
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .border_1()
                                    .border_color(rgb(colors.muted))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_sm()
                                    .text_color(rgb(colors.muted))
                                    .cursor_pointer()
                                    .child("?")
                                    .hover(move |style| {
                                        style
                                            .bg(rgb(colors.button_hover))
                                            .text_color(rgb(colors.accent))
                                    })
                                    .on_click(cx.listener(move |app, _, window, cx| {
                                        app.open_modal(
                                            Modal::SyncOptionsHelp(selection),
                                            window,
                                            cx,
                                        )
                                    })),
                            )
                        }
                        _ => None,
                    })
                    .child(self.icon_button("close-modal", "✕", false).on_click(
                        cx.listener(|app, _, window, cx| app.dismiss(&Dismiss, window, cx)),
                    )),
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
            Modal::ConfirmUpdate => {
                card = card
                    .child(
                        self.button(
                            "confirm-application-update",
                            &localized_format!("Да, обновить", "Yes, update"),
                            true,
                        )
                        .on_click(cx.listener(|app, _, _, cx| app.download_application_update(cx))),
                    )
                    .child(
                        self.button("cancel-application-update", tr("Отмена"), false)
                            .on_click(
                                cx.listener(|app, _, window, cx| app.dismiss(&Dismiss, window, cx)),
                            ),
                    );
            }
            Modal::DeleteIgnoredTracks(tracks) => {
                card = card
                    .child(
                        self.button(
                            "confirm-delete-ignored",
                            tr("Удалить все игнорируемые треки"),
                            false,
                        )
                        .text_color(rgb(0xf29a9a))
                        .on_click(cx.listener(
                            move |app, _, window, cx| {
                                app.delete_ignored_tracks(tracks.clone(), cx);
                                window.focus(&app.focus);
                            },
                        )),
                    )
                    .child(
                        self.button("cancel-delete-ignored", tr("Отмена"), false)
                            .on_click(
                                cx.listener(|app, _, window, cx| app.dismiss(&Dismiss, window, cx)),
                            ),
                    );
            }
            Modal::SyncOptions(selection) => {
                card = card.child(self.sync_options_view(selection, cx));
            }
            Modal::SyncOptionsHelp(selection) => {
                card = card.child(self.sync_options_help()).child(
                    self.button("back-sync-options", tr("Понятно"), true)
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.open_modal(Modal::SyncOptions(selection), window, cx)
                        })),
                );
            }
            Modal::SyncDuplicates => {
                card = card.child(self.sync_duplicates_view(cx));
            }
            Modal::SyncHelp => {
                card = card.child(self.sync_help()).child(
                    self.button("close-sync-help", tr("Понятно"), true)
                        .on_click(
                            cx.listener(|app, _, window, cx| app.dismiss(&Dismiss, window, cx)),
                        ),
                );
            }
            Modal::ClearStorage => {
                card = card.child(
                    self.button(
                        "confirm-clear-storage",
                        &localized_format!("Удалить всю музыку", "Delete all music"),
                        false,
                    )
                    .text_color(rgb(0xf29a9a))
                    .on_click(cx.listener(|app, _, _, cx| app.clear_storage(cx))),
                );
            }
            Modal::MoveStorage(destination) => {
                card = card.child(
                    self.button(
                        "confirm-storage-move",
                        tr("Перенести и перезапустить"),
                        true,
                    )
                    .on_click(
                        cx.listener(move |app, _, _, cx| app.move_storage(destination.clone(), cx)),
                    ),
                );
            }
            Modal::Downloads => {
                card = card.child(self.downloads_view(cx)).child(
                    self.button(
                        "downloads-add",
                        &localized_format!("+ Добавить музыку", "+ Add music"),
                        true,
                    )
                    .on_click(
                        cx.listener(|app, _, window, cx| app.open_modal(Modal::Import, window, cx)),
                    ),
                );
            }
            Modal::MergePlaylist(source) => {
                let first = source.id;
                let source_name = source.name.clone();
                card = card.child(
                    div()
                        .id("merge-targets")
                        .max_h(px(300.))
                        .overflow_y_scroll()
                        .children(self.playlists.iter().filter(|p| p.id != first).map(
                            |playlist| {
                                let second = playlist.id;
                                let name: String = format!("{} + {}", source_name, playlist.name)
                                    .chars()
                                    .take(100)
                                    .collect();
                                div()
                                    .id(("merge-with", second as u64))
                                    .p_3()
                                    .mb_2()
                                    .rounded_lg()
                                    .bg(rgb(colors.button))
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(rgb(colors.button_hover)))
                                    .child(playlist.name.clone())
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        let service = app.playlists_service.clone();
                                        let name = name.clone();
                                        app.run_job(
                                            tr("Сохраняем…"),
                                            move || service.merge_playlists(first, second, &name),
                                            |app, id, _| {
                                                app.selected_playlist = Some(id);
                                                app.home_open = false;
                                                app.settings_open = false;
                                                app.modal = None;
                                            },
                                            cx,
                                        );
                                    }))
                            },
                        )),
                );
            }
            Modal::PlaylistActions(playlist) => {
                let id = playlist.id;
                let merge = playlist.clone();
                card = card
                    .child(
                        self.button("merge-playlists", tr("Объединить плейлисты"), false)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.open_modal(Modal::MergePlaylist(merge.clone()), window, cx)
                            })),
                    )
                    .child(
                        self.button("menu-pick-tracks", tr("+  Треки"), false)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.open_modal(Modal::PickTracks(id), window, cx)
                            })),
                    )
                    .children((id != shard::favorites::FAVORITES_ID).then(|| {
                        self.button("menu-delete-playlist", tr("Удалить плейлист"), false)
                            .text_color(rgb(0xf1aaa0))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.open_modal(Modal::DeletePlaylist(playlist.clone()), window, cx)
                            }))
                    }));
                if self.sync_service.is_some() {
                    card = card.child(
                        self.button("sync-playlist", tr("Синхронизировать плейлист"), false)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.request_sync(
                                    shard::sync::SyncSelection::Playlist(id),
                                    window,
                                    cx,
                                )
                            })),
                    );
                }
            }
            Modal::EditTrack(_) => {
                card = card
                    .child(div().text_sm().child(
                        if matches!(self.modal, Some(Modal::EditTrack(_))) {
                            tr("Название трека")
                        } else {
                            tr("Название плейлиста")
                        },
                    ))
                    .child(self.input.clone());
                if matches!(self.modal, Some(Modal::EditTrack(_))) {
                    card = card
                        .child(div().text_sm().child(tr("Исполнитель")))
                        .child(self.artist_input.clone());
                }
                card = card.child(
                    self.button("save-metadata", tr("Сохранить"), true)
                        .on_click(
                            cx.listener(|app, _, window, cx| app.submit(&Submit, window, cx)),
                        ),
                );
            }
            Modal::DeletePlaylist(playlist) => {
                card = card
                    .child(
                        self.button("confirm-delete-playlist", tr("Удалить плейлист"), false)
                            .text_color(rgb(0xf1aaa0))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.delete_playlist(playlist.clone(), cx);
                                window.focus(&app.focus);
                            })),
                    )
                    .child(
                        self.button("cancel-delete-playlist", tr("Отмена"), false)
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
                            .child(self.playlist_artwork(playlist, 72.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(colors.muted))
                                            .child(tr("Название плейлиста")),
                                    )
                                    .child(self.playlist_name.clone()),
                            ),
                    );
                }
                card = card
                    .child(
                        self.button("choose-cover", tr("Выбрать обложку…"), true)
                            .on_click(cx.listener(move |app, _, _, cx| app.choose_cover(id, cx))),
                    )
                    .child(self.header_color_controls(id, cx))
                    .child(
                        self.button("remove-cover", tr("Убрать обложку"), false)
                            .on_click(cx.listener(move |app, _, _, cx| {
                                let service = app.appearance_service.clone();
                                app.run_job(
                                    tr("Убираем обложку…"),
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
                        self.button("local-file", tr("↗   Выбрать аудиофайлы"), false).on_click(
                            cx.listener(|app, _, window, cx| app.import_local(window, cx)),
                        ),
                    )
                    .child(self.input.clone())
                    .child(self.button("download", if self.active_download.is_some() { tr("Добавить в очередь") } else { tr("Скачать и добавить") }, true).on_click(
                        cx.listener(|app, _, window, cx| app.submit(&Submit, window, cx)),
                    ))
                    .child(self.button("show-downloads", tr("Загрузки"), false)
                        .on_click(cx.listener(|app, _, window, cx| app.open_modal(Modal::Downloads, window, cx))))
                    .child(div().text_xs().text_color(rgb(colors.muted)).child(
                        tr("Плейлист сохранится в приложении. Треки Spotify и Яндекс Музыки ищутся на YouTube по названию и исполнителю."),
                    ));
            }
            Modal::CreatePlaylist => {
                card = card.child(self.input.clone()).child(
                    self.button("create", tr("Создать плейлист"), true)
                        .on_click(
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
                            .child(tr("Пока нет плейлистов. Создайте первый кнопкой ниже.")),
                    );
                }
                card = card.child(
                    self.button("create-first", tr("+  Новый плейлист"), false)
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
                            .child(tr("Сначала добавьте музыку в общую библиотеку.")),
                    );
                }
            }
            Modal::TrackActions(track) => {
                let ignore_id = track.id;
                let ignored = self.ignored_ids.contains(&ignore_id);
                let edit = track.clone();
                let queued = track.clone();
                let add = track.clone();
                let remove = track.clone();
                let sync_track = track.id;
                let delete = track;
                card = card
                    .child(
                        self.button(
                            "ignore-track",
                            if ignored {
                                tr("Убрать из игнора")
                            } else {
                                tr("Игнорировать трек")
                            },
                            false,
                        )
                        .on_click(cx.listener(
                            move |app, _, window, cx| {
                                app.set_track_ignored(ignore_id, !ignored, cx);
                                app.modal = None;
                                window.focus(&app.focus);
                            },
                        )),
                    )
                    .child(
                        self.button("edit-track", tr("Изменить название и исполнителя"), false)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.open_modal(Modal::EditTrack(edit.clone()), window, cx)
                            })),
                    )
                    .child(
                        self.button("enqueue-track", tr("≡  Добавить в очередь"), false)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.enqueue_track(&queued, cx);
                                app.modal = None;
                                window.focus(&app.focus);
                            })),
                    )
                    .child(
                        self.button("action-add-playlist", tr("+  Добавить в плейлист"), false)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.open_modal(Modal::AddToPlaylist(add.clone()), window, cx)
                            })),
                    );
                if self.sync_service.is_some() {
                    card = card.child(
                        self.button("sync-track", tr("Синхронизировать трек"), false)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.request_sync(
                                    shard::sync::SyncSelection::Track(sync_track),
                                    window,
                                    cx,
                                )
                            })),
                    );
                }
                if self.selected_playlist.is_some() {
                    card = card.child(
                        self.button(
                            "remove-from-playlist",
                            tr("−  Убрать из этого плейлиста"),
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
                    self.button("delete-from-storage", tr("Удалить из хранилища…"), false)
                        .text_color(rgb(0xf1aaa0))
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.open_modal(Modal::DeleteTrack(delete.clone()), window, cx)
                        })),
                );
            }
            Modal::DeleteTrack(track) => {
                card = card
                    .child(
                        self.button("confirm-delete-track", tr("Удалить файл и трек"), false)
                            .bg(rgb(0x713e37))
                            .text_color(rgb(0xffe3dd))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.delete_track(track.clone(), cx);
                                window.focus(&app.focus);
                            })),
                    )
                    .child(
                        self.button("cancel-delete-track", tr("Отмена"), false)
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
                self.button("cancel-download", tr("Остановить загрузку"), false)
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
                .on_mouse_move(cx.listener(|app, event: &MouseMoveEvent, _, cx| {
                    if let Some(kind) = app.dragging_slider {
                        app.change_slider(kind, event.position, cx);
                    }
                }))
                .bg(rgba(0x060a08cc))
                .flex()
                .items_center()
                .justify_center()
                .child(card.with_animation(
                    "modal-enter",
                    Animation::new(Duration::from_millis(160)).with_easing(ease_out_quint()),
                    |card, t| card.opacity(t),
                ))
                .into_any_element(),
        )
    }
}

impl MusicApp {
    fn button(&self, id: impl Into<ElementId>, label: &str, primary: bool) -> Stateful<Div> {
        let colors = self.colors();
        div()
            .id(id)
            .relative()
            .px_4()
            .py_3()
            .flex_shrink_0()
            .rounded_xl()
            .border_1()
            .border_color(rgb(if primary {
                colors.accent
            } else {
                colors.border
            }))
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
            .active(|s| s.opacity(0.72))
            .child(self.button_feedback(12.))
            .child(label.to_owned())
    }
}

fn format_duration(ms: Option<i64>) -> String {
    match ms {
        Some(ms) => format!("{}:{:02}", ms.max(0) / 60_000, ms.max(0) / 1000 % 60),
        None => "—".into(),
    }
}
