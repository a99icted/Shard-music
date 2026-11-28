use crate::i18n::tr;
use crate::localized_format;
use crate::youtube::{
    DownloadInfo, YoutubeClient, YoutubeDownloadReport, YoutubeFailure, YoutubePlaylist,
    YoutubeProgress, YoutubeSource, playlist_url,
};
use crate::{
    db::Database,
    models::{NewTrack, Playlist, Track},
    repositories::{PlaylistRepository, TrackRepository},
};
use anyhow::{Context, Result, ensure};
use lofty::{
    file::{AudioFile, TaggedFileExt},
    prelude::Accessor,
    probe::Probe,
};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "opus", "m4a", "aac", "aiff", "aif", "mp4",
];

// Downloads and LAN transfers offer the same duplicate decisions and defaults.
pub use crate::sync::SyncOptions as DownloadOptions;
use crate::sync::{DuplicateDecision, DuplicateKind, DuplicateTrack};

#[derive(Clone)]
pub struct TrackService {
    db: Database,
    repository: TrackRepository,
}

impl TrackService {
    pub fn download_search_collection_with_options(
        &self,
        collection: &crate::youtube::search::CollectionDownload,
        cancel: &AtomicBool,
        progress: impl FnMut(YoutubeProgress),
        options: DownloadOptions,
        mut decide: impl FnMut(&[DuplicateTrack]) -> Result<Vec<DuplicateDecision>>,
    ) -> Result<YoutubeDownloadReport> {
        self.download_search_collection_with_source(
            collection,
            &YoutubeClient::new(self.db.root())?,
            cancel,
            progress,
            options,
            &mut decide,
        )
    }

    fn download_search_collection_with_source(
        &self,
        collection: &crate::youtube::search::CollectionDownload,
        source: &impl YoutubeSource,
        cancel: &AtomicBool,
        progress: impl FnMut(YoutubeProgress),
        options: DownloadOptions,
        decide: &mut impl FnMut(&[DuplicateTrack]) -> Result<Vec<DuplicateDecision>>,
    ) -> Result<YoutubeDownloadReport> {
        use crate::youtube::{
            YoutubeEntry,
            search::{CollectionSource, ResultKind},
        };
        ensure!(
            collection.collection.kind != ResultKind::Track,
            "Expected an album or playlist"
        );
        ensure!(
            !collection.tracks.is_empty(),
            "The collection has no available tracks"
        );
        let entries = collection
            .tracks
            .iter()
            .map(|track| {
                ensure!(
                    track.kind == ResultKind::Track,
                    "Expected a track in the collection"
                );
                Ok(YoutubeEntry {
                    title: track.title.clone(),
                    artist: track.artist.clone(),
                    duration_ms: track.duration_ms,
                    url: Some(validate_youtube_url(&track.url)?),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let source = CollectionSource {
            source,
            collection: &collection.collection,
        };
        self.download_entries(
            YoutubePlaylist {
                title: collection.collection.title.clone(),
                entries,
            },
            true,
            &source,
            cancel,
            progress,
            options,
            decide,
        )
    }

    pub fn download_collection(
        &self,
        input: &str,
        cancel: &AtomicBool,
        progress: impl FnMut(YoutubeProgress),
    ) -> Result<YoutubeDownloadReport> {
        self.download_collection_with_options(
            input,
            cancel,
            progress,
            DownloadOptions {
                warn_duplicates: false,
                ignore_duplicates: false,
            },
            |_| unreachable!(),
        )
    }

    pub fn download_collection_with_options(
        &self,
        input: &str,
        cancel: &AtomicBool,
        mut progress: impl FnMut(YoutubeProgress),
        options: DownloadOptions,
        mut decide: impl FnMut(&[DuplicateTrack]) -> Result<Vec<DuplicateDecision>>,
    ) -> Result<YoutubeDownloadReport> {
        let url = validate_music_url(input)?;
        if !matches!(
            url.host_str(),
            Some(
                "open.spotify.com"
                    | "music.yandex.ru"
                    | "music.yandex.com"
                    | "music.yandex.kz"
                    | "music.yandex.by"
                    | "music.yandex.uz"
            )
        ) {
            let url = validate_youtube_url(input)?;
            let source = YoutubeClient::new(self.db.root())?;
            return self.download_youtube_with_options(
                &url,
                &source,
                cancel,
                progress,
                options,
                &mut decide,
            );
        }
        progress(YoutubeProgress {
            completed: 0,
            total: 0,
            current_title: None,
        });
        let metadata = if url.host_str() == Some("open.spotify.com") {
            crate::spotify::fetch(&url, cancel)
        } else {
            crate::yandex::fetch(&url, cancel)
        };
        let collection = match metadata {
            Err(_) if cancel.load(Ordering::Relaxed) => {
                return Ok(YoutubeDownloadReport {
                    cancelled: true,
                    ..Default::default()
                });
            }
            result => result?,
        };
        let source = crate::music_metadata::MetadataSource {
            youtube: YoutubeClient::new(self.db.root())?,
            collection,
        };
        self.download_entries(
            source.entries(),
            source.collection.is_collection,
            &source,
            cancel,
            progress,
            options,
            &mut decide,
        )
    }
    pub fn new(db: Database) -> Self {
        Self {
            repository: TrackRepository::new(db.clone()),
            db,
        }
    }

    pub fn list_tracks(&self) -> Result<Vec<Track>> {
        self.repository.list_tracks()
    }

    /// Backfill existing libraries without re-downloading any audio.
    pub fn refresh_artwork(&self, cancel: &AtomicBool) -> Result<usize> {
        let mut updated = 0;
        for track in self.list_tracks()? {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let destination = crate::artwork::track_path(self.db.root(), &track);
            if destination.is_file() {
                continue;
            }
            let embedded = self
                .audio_path(&track)
                .ok()
                .and_then(|path| Probe::open(path).ok())
                .and_then(|probe| probe.read().ok())
                .and_then(|file| {
                    file.primary_tag()
                        .or_else(|| file.first_tag())
                        .and_then(|tag| tag.pictures().first())
                        .map(|picture| picture.data().to_vec())
                });
            let remote = || -> Result<Vec<u8>> {
                let url = url::Url::parse(track.source_url.as_deref().context("No source")?)?;
                let host = url.host_str().unwrap_or_default();
                let artwork = if host == "open.spotify.com" || crate::yandex::is_host(host) {
                    let collection = if host == "open.spotify.com" {
                        crate::spotify::fetch(&url, cancel)?
                    } else {
                        crate::yandex::fetch(&url, cancel)?
                    };
                    collection
                        .artwork_url
                        .or_else(|| {
                            collection
                                .tracks
                                .into_iter()
                                .find_map(|track| track.artwork_url)
                        })
                        .context("No artwork")?
                } else {
                    let url = validate_youtube_url(url.as_str())?;
                    let id = if url.host_str() == Some("youtu.be") {
                        url.path().trim_start_matches('/').to_owned()
                    } else {
                        url.query_pairs()
                            .find(|(key, _)| key == "v")
                            .map(|(_, value)| value.into_owned())
                            .context("No video ID")?
                    };
                    ensure!(
                        id.bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
                        "Invalid video ID"
                    );
                    format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")
                };
                crate::artwork::download(&artwork)
            };
            if let Some(bytes) = embedded.or_else(|| remote().ok()) {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                if crate::artwork::save(&destination, &bytes).is_ok() {
                    updated += 1;
                }
            }
        }
        let appearance = crate::appearance::AppearanceService::new(self.db.clone());
        for playlist in PlaylistService::new(self.db.clone()).list_playlists()? {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if playlist.cover_path.is_none() {
                if let Some(track) = self.repository_for_playlist_cover(playlist.id)? {
                    let _ = appearance.set_cover_if_missing(
                        playlist.id,
                        &crate::artwork::track_path(self.db.root(), &track),
                    );
                }
            }
        }
        Ok(updated)
    }

    fn repository_for_playlist_cover(&self, id: i64) -> Result<Option<Track>> {
        Ok(PlaylistRepository::new(self.db.clone())
            .list_playlist_tracks(id)?
            .into_iter()
            .find(|track| crate::artwork::track_path(self.db.root(), track).is_file()))
    }

    pub fn audio_path(&self, track: &Track) -> Result<PathBuf> {
        let relative = Path::new(&track.file_path);
        ensure!(
            relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
                && relative.starts_with("audio"),
            tr("Некорректный путь аудиофайла в базе")
        );
        Ok(self.db.root().join(relative))
    }

    pub fn import_local(&self, path: &Path) -> Result<Track> {
        self.import_file(path, None, None)
    }

    /// Platform download adapters supply a completed file, preserving the same
    /// metadata validation and atomic library import as the desktop downloader.
    pub fn import_download(
        &self,
        path: &Path,
        source: &str,
        title: Option<String>,
        artist: Option<String>,
        thumbnail: Option<String>,
    ) -> Result<Track> {
        let source = validate_music_url(source)?;
        self.import_file(
            path,
            Some(source.to_string()),
            Some(DownloadInfo {
                track: title,
                artist,
                thumbnail,
                ..Default::default()
            }),
        )
    }

    pub fn update_metadata(&self, id: i64, title: &str, artist: &str) -> Result<Track> {
        let title = title.trim();
        let artist = artist.trim();
        ensure!(!title.is_empty(), tr("Введите название трека"));
        ensure!(
            title.chars().count() <= 500 && artist.chars().count() <= 500,
            tr("Не больше 500 символов в каждом поле")
        );
        self.repository.update_metadata(id, title, artist)
    }

    pub fn delete_track(&self, id: i64) -> Result<()> {
        let track = self.repository.get(id)?;
        let original = self.audio_path(&track)?;
        let pending = self
            .db
            .root()
            .join("staging")
            .join(format!("delete-{}", Uuid::new_v4()));
        let moved = match fs::rename(&original, &pending) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => {
                return Err(error).context(tr("Не удалось подготовить аудиофайл к удалению"));
            }
        };
        if let Err(error) = self.repository.delete(id) {
            if moved {
                fs::rename(&pending, &original).with_context(|| {
                    localized_format!(
                        "Не удалось восстановить аудиофайл после ошибки базы: {}",
                        "Could not restore audio file after database error: {}",
                        pending.display()
                    )
                })?;
            }
            return Err(error);
        }
        let _ = fs::remove_file(crate::artwork::track_path(self.db.root(), &track));
        let _ = fs::remove_file(crate::artwork::thumbnail_path(&crate::artwork::track_path(
            self.db.root(),
            &track,
        )));
        if moved {
            fs::remove_file(&pending).with_context(|| {
                localized_format!(
                    "Запись удалена, но не удалось очистить аудиофайл: {}",
                    "Entry deleted, but could not remove audio file: {}",
                    pending.display()
                )
            })?;
        }
        Ok(())
    }

    fn import_file(
        &self,
        source: &Path,
        source_url: Option<String>,
        info: Option<DownloadInfo>,
    ) -> Result<Track> {
        ensure!(
            source.is_file(),
            "{}",
            localized_format!(
                "Аудиофайл не найден: {}",
                "Audio file not found: {}",
                source.display()
            )
        );
        let extension = source
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        ensure!(
            AUDIO_EXTENSIONS.contains(&extension.as_str()),
            "{}",
            localized_format!(
                "Неподдерживаемый формат аудио: {extension}",
                "Unsupported audio format: {extension}"
            )
        );
        // Parse before copying: failed imports leave no entry or orphan file.
        let info = info.unwrap_or_default();
        let artwork_url = info.thumbnail.clone();
        let artwork_fallback = info.thumbnail_fallback.clone();
        let (title, artist, duration_ms, embedded) = audio_metadata(source, &info)?;
        let relative = format!("audio/{}.{}", Uuid::new_v4(), extension);
        let destination = self.db.root().join(&relative);
        let mut staged = tempfile::NamedTempFile::new_in(self.db.root().join("staging"))?;
        let mut original = fs::File::open(source)?;
        std::io::copy(&mut original, staged.as_file_mut())?;
        staged.as_file().sync_all()?;
        staged
            .persist_noclobber(&destination)
            .context(tr("Не удалось сохранить аудиофайл"))?;
        let result = self.repository.insert(&NewTrack {
            title,
            artist,
            source_url,
            file_path: relative,
            duration_ms,
        });
        if result.is_err() {
            let _ = fs::remove_file(destination);
        }
        if let Ok(track) = &result {
            let cover = crate::artwork::track_path(self.db.root(), track);
            // Metadata services provide the album cover; embedded art is the offline fallback.
            let downloaded = artwork_url
                .as_deref()
                .into_iter()
                .chain(artwork_fallback.as_deref())
                .any(|url| {
                    crate::artwork::download(url)
                        .is_ok_and(|bytes| crate::artwork::save(&cover, &bytes).is_ok())
                });
            if !downloaded {
                if let Some(bytes) = embedded {
                    let _ = crate::artwork::save(&cover, &bytes);
                }
            }
        }
        result
    }

    pub fn download_youtube_collection(
        &self,
        input: &str,
        cancel: &AtomicBool,
        progress: impl FnMut(YoutubeProgress),
    ) -> Result<YoutubeDownloadReport> {
        let url = validate_youtube_url(input)?;
        let source = YoutubeClient::new(self.db.root())?;
        self.download_youtube_with_source(&url, &source, cancel, progress)
    }

    fn download_youtube_with_source(
        &self,
        url: &url::Url,
        source: &impl YoutubeSource,
        cancel: &AtomicBool,
        progress: impl FnMut(YoutubeProgress),
    ) -> Result<YoutubeDownloadReport> {
        self.download_youtube_with_options(
            url,
            source,
            cancel,
            progress,
            DownloadOptions {
                warn_duplicates: false,
                ignore_duplicates: false,
            },
            &mut |_| unreachable!(),
        )
    }

    fn download_youtube_with_options(
        &self,
        url: &url::Url,
        source: &impl YoutubeSource,
        cancel: &AtomicBool,
        mut progress: impl FnMut(YoutubeProgress),
        options: DownloadOptions,
        decide: &mut impl FnMut(&[DuplicateTrack]) -> Result<Vec<DuplicateDecision>>,
    ) -> Result<YoutubeDownloadReport> {
        let mut report = YoutubeDownloadReport::default();
        progress(YoutubeProgress {
            completed: 0,
            total: 0,
            current_title: None,
        });
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            return Ok(report);
        }
        let playlist_link = playlist_url(url);
        let playlist = if let Some(link) = &playlist_link {
            match source.playlist(link, cancel) {
                Ok(playlist) => playlist,
                Err(_) if cancel.load(Ordering::Relaxed) => {
                    report.cancelled = true;
                    return Ok(report);
                }
                Err(error) => return Err(error),
            }
        } else {
            YoutubePlaylist {
                title: String::new(),
                entries: vec![match source.entry(url, cancel) {
                    Ok(entry) => entry,
                    Err(_) if cancel.load(Ordering::Relaxed) => {
                        report.cancelled = true;
                        return Ok(report);
                    }
                    Err(error) => return Err(error),
                }],
            }
        };
        self.download_entries(
            playlist,
            playlist_link.is_some(),
            source,
            cancel,
            progress,
            options,
            decide,
        )
    }

    fn download_entries(
        &self,
        playlist: YoutubePlaylist,
        is_playlist: bool,
        source: &impl YoutubeSource,
        cancel: &AtomicBool,
        mut progress: impl FnMut(YoutubeProgress),
        options: DownloadOptions,
        decide: &mut impl FnMut(&[DuplicateTrack]) -> Result<Vec<DuplicateDecision>>,
    ) -> Result<YoutubeDownloadReport> {
        let mut report = YoutubeDownloadReport {
            total: playlist.entries.len(),
            ..Default::default()
        };
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            return Ok(report);
        }
        let local = self.download_candidates()?;
        let entries = playlist
            .entries
            .iter()
            .map(|entry| crate::download_duplicates::Entry {
                url: entry
                    .url
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                title: entry.title.clone(),
                artist: entry.artist.clone(),
                duration_ms: entry.duration_ms,
            })
            .collect::<Vec<_>>();
        let duplicates = crate::download_duplicates::inspect(&local, &entries);
        let prompts = duplicates
            .iter()
            .map(|d| DuplicateTrack {
                index: d.index,
                title: entries[d.index].title.clone(),
                artist: entries[d.index].artist.clone(),
                existing_title: d.existing_title.clone(),
                kind: if d.kind == "same_source" {
                    DuplicateKind::SameSource
                } else {
                    DuplicateKind::SimilarMetadata
                },
            })
            .collect::<Vec<_>>();
        let decisions = match review_download_duplicates(&prompts, options, decide) {
            Err(_) if cancel.load(Ordering::Relaxed) => {
                report.cancelled = true;
                return Ok(report);
            }
            result => result?,
        };
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            return Ok(report);
        }
        let mut resolved = vec![None::<Track>; entries.len()];
        let playlists = PlaylistService::new(self.db.clone());
        if is_playlist {
            let name: String = playlist.title.trim().chars().take(100).collect();
            report.playlist_id = Some(playlists.create_playlist(if name.is_empty() {
                tr("Плейлист YouTube")
            } else {
                &name
            })?);
        }
        if let Some(id) = report.playlist_id {
            if let Some(url) = source.artwork_url() {
                if let Ok(bytes) = crate::artwork::download(url) {
                    if let Ok(staging) = tempfile::tempdir_in(self.db.root().join("staging")) {
                        let path = staging.path().join("cover.png");
                        if crate::artwork::save(&path, &bytes).is_ok() {
                            let _ = crate::appearance::AppearanceService::new(self.db.clone())
                                .set_cover_if_missing(id, &path);
                        }
                    }
                }
            }
        }
        for (index, entry) in playlist.entries.into_iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                break;
            }
            progress(YoutubeProgress {
                completed: index,
                total: report.total,
                current_title: Some(entry.title.clone()),
            });
            let mut reused = false;
            let result = (|| -> Result<Track> {
                let duplicate = duplicates.iter().find(|d| d.index == index);
                let copy = decisions.get(&index).copied();
                if copy == Some(false) {
                    let existing = duplicate
                        .and_then(|d| {
                            d.existing_id
                                .and_then(|id| self.repository.get(id).ok())
                                .or_else(|| d.earlier_index.and_then(|i| resolved[i].clone()))
                        })
                        .filter(|t| self.audio_path(t).is_ok_and(|p| p.is_file()));
                    if let Some(track) = existing {
                        reused = true;
                        return Ok(track);
                    }
                    // An earlier entry may have failed or a local file may have disappeared.
                    // Download this entry rather than silently losing it from the playlist.
                }
                let url = entry.url.context(tr(
                    "YouTube не предоставил ссылку на этот трек (видео удалено или скрыто)",
                ))?;
                let staging = tempfile::tempdir_in(self.db.root().join("staging"))?;
                let audio = source.track(&url, staging.path(), cancel)?;
                ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
                // A source may expose richer metadata only after downloading. Also check
                // byte-identical local audio, using the same cached hashes as LAN sync.
                if copy != Some(true) && (options.warn_duplicates || options.ignore_duplicates) {
                    if let Some((track, kind)) = self.downloaded_duplicate(&audio, &url, cancel)? {
                        let prompt = DuplicateTrack {
                            index,
                            title: entry.title.clone(),
                            artist: entry.artist.clone(),
                            existing_title: track.title.clone(),
                            kind,
                        };
                        let late = review_download_duplicates(&[prompt], options, decide)?;
                        ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
                        if late.get(&index) == Some(&false) {
                            reused = true;
                            return Ok(track);
                        }
                    }
                }
                self.import_file(&audio.path, Some(url.to_string()), Some(audio.info))
            })();
            match result {
                Ok(track) => {
                    resolved[index] = Some(track.clone());
                    if reused {
                        report.reused += 1;
                    }
                    if let Some(id) = report.playlist_id
                        && let Err(error) = playlists.add_track(id, track.id)
                    {
                        report.failures.push(YoutubeFailure {
                            title: track.title.clone(),
                            message: format!(
                                "Трек сохранён в библиотеку, но не добавлен в плейлист: {error:#}"
                            ),
                        });
                    }
                    if let Some(id) = report.playlist_id {
                        let cover = crate::artwork::track_path(self.db.root(), &track);
                        let _ = crate::appearance::AppearanceService::new(self.db.clone())
                            .set_cover_if_missing(id, &cover);
                    }
                    if !reused {
                        report.tracks.push(track);
                    }
                }
                Err(_) if cancel.load(Ordering::Relaxed) => {
                    report.cancelled = true;
                    break;
                }
                Err(error) => report.failures.push(YoutubeFailure {
                    title: entry.title,
                    message: format!("{error:#}"),
                }),
            }
            progress(YoutubeProgress {
                completed: index + 1,
                total: report.total,
                current_title: None,
            });
        }
        Ok(report)
    }

    fn download_candidates(&self) -> Result<Vec<Track>> {
        Ok(self
            .list_tracks()?
            .into_iter()
            .filter(|t| self.audio_path(t).is_ok_and(|p| p.is_file()))
            .collect())
    }

    fn downloaded_duplicate(
        &self,
        audio: &crate::youtube::DownloadedAudio,
        url: &url::Url,
        cancel: &AtomicBool,
    ) -> Result<Option<(Track, DuplicateKind)>> {
        let (title, artist, duration_ms, _) = audio_metadata(&audio.path, &audio.info)?;
        let entry = crate::download_duplicates::Entry {
            url: url.to_string(),
            title,
            artist,
            duration_ms,
        };
        let local = self.download_candidates()?;
        let size = fs::metadata(&audio.path)?.len();
        let mut hash = None;
        let conn = self.db.connect()?;
        for track in &local {
            ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
            let path = self.audio_path(track)?;
            if !fs::metadata(&path).is_ok_and(|m| m.len() == size) {
                continue;
            }
            if hash.is_none() {
                hash = Some(crate::sync::hash_file(&audio.path, || {
                    ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
                    Ok(())
                })?);
            }
            let existing = crate::sync::indexed_hash(&conn, track, &path, || {
                ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
                Ok(())
            });
            if existing.as_ref().ok() == hash.as_ref() {
                return Ok(Some((track.clone(), DuplicateKind::IdenticalFile)));
            }
        }
        Ok(crate::download_duplicates::inspect(&local, &[entry])
            .first()
            .and_then(|d| {
                local.iter().find(|t| Some(t.id) == d.existing_id).map(|t| {
                    (
                        t.clone(),
                        if d.kind == "same_source" {
                            DuplicateKind::SameSource
                        } else {
                            DuplicateKind::SimilarMetadata
                        },
                    )
                })
            }))
    }
}

fn review_download_duplicates(
    tracks: &[DuplicateTrack],
    options: DownloadOptions,
    decide: &mut impl FnMut(&[DuplicateTrack]) -> Result<Vec<DuplicateDecision>>,
) -> Result<std::collections::HashMap<usize, bool>> {
    let mut result = std::collections::HashMap::new();
    if tracks.is_empty() {
        return Ok(result);
    }
    if options.warn_duplicates {
        for decision in decide(tracks)? {
            ensure!(
                tracks.iter().any(|t| t.index == decision.index)
                    && result.insert(decision.index, decision.copy).is_none(),
                "Invalid duplicate decision"
            );
        }
        ensure!(result.len() == tracks.len(), "Missing duplicate decisions");
    } else {
        result.extend(tracks.iter().map(|t| (t.index, !options.ignore_duplicates)));
    }
    Ok(result)
}

fn audio_metadata(
    source: &Path,
    info: &DownloadInfo,
) -> Result<(String, String, Option<i64>, Option<Vec<u8>>)> {
    let tagged = Probe::open(source)?
        .read()
        .context(tr("Не удалось прочитать аудиофайл"))?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    let embedded = tag
        .and_then(|tag| tag.pictures().first())
        .map(|p| p.data().to_vec());
    let title = info
        .track
        .clone()
        .or_else(|| {
            tag.and_then(|t| t.title())
                .map(|s| s.into_owned())
                .filter(|s| !s.trim().is_empty())
        })
        .or(info.title.clone())
        .unwrap_or_else(|| {
            source
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
    let artist = info
        .artist
        .clone()
        .or_else(|| {
            tag.and_then(|t| t.artist())
                .map(|s| s.into_owned())
                .filter(|s| !s.trim().is_empty())
        })
        .or(info.uploader.clone())
        .unwrap_or_default();
    let duration = tagged.properties().duration().as_millis();
    let duration_ms = if duration > 0 {
        Some(i64::try_from(duration)?)
    } else {
        info.duration
            .filter(|v| v.is_finite() && *v >= 0.0)
            .map(|v| (v * 1000.0) as i64)
    };
    Ok((title, artist, duration_ms, embedded))
}

pub fn validate_music_url(input: &str) -> Result<url::Url> {
    let host = url::Url::parse(input.trim())
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .as_deref()
        .map(str::to_owned);
    if host.as_deref().is_some_and(crate::yandex::is_host) {
        crate::yandex::validate_yandex_url(input)
    } else if host.as_deref() == Some("open.spotify.com") {
        crate::spotify::validate_spotify_url(input)
    } else {
        validate_youtube_url(input).context(tr(
            "Вставьте ссылку YouTube, Spotify или Яндекс Музыки на трек или открытый плейлист",
        ))
    }
}

pub fn validate_youtube_url(input: &str) -> Result<url::Url> {
    let url = url::Url::parse(input.trim())
        .context(tr("Вставьте полную ссылку YouTube, начиная с https://"))?;
    ensure!(
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none(),
        tr("Нужна HTTPS-ссылка YouTube")
    );
    ensure!(
        matches!(
            url.host_str(),
            Some(
                "youtube.com"
                    | "www.youtube.com"
                    | "m.youtube.com"
                    | "music.youtube.com"
                    | "youtu.be"
            )
        ),
        tr("Поддерживаются ссылки youtube.com и youtu.be")
    );
    ensure!(
        url.path() != "/" && !url.path().is_empty(),
        tr("Укажите ссылку на трек, видео или плейлист")
    );
    Ok(url)
}

#[derive(Clone)]
pub struct PlaylistService {
    repository: PlaylistRepository,
    db: Database,
}

impl PlaylistService {
    pub fn merge_playlists(&self, first: i64, second: i64, name: &str) -> Result<i64> {
        ensure!(
            first != second,
            "{}",
            localized_format!(
                "Выберите два разных плейлиста",
                "Choose two different playlists"
            )
        );
        let name = name.trim();
        ensure!(
            !name.is_empty() && name.chars().count() <= 100,
            "{}",
            localized_format!(
                "Название плейлиста должно содержать от 1 до 100 символов",
                "Playlist name must contain 1–100 characters"
            )
        );
        let mut conn = self.db.connect()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut seen = std::collections::HashSet::new();
        let mut tracks = Vec::new();
        for id in [first, second] {
            let sql = if id == crate::favorites::FAVORITES_ID {
                "SELECT track_id FROM favorites ORDER BY added_at,track_id"
            } else {
                ensure!(
                    tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM playlists WHERE id=?1)",
                        [id],
                        |r| r.get::<_, bool>(0)
                    )?,
                    "{}",
                    tr("Плейлист не найден")
                );
                "SELECT track_id FROM playlist_tracks WHERE playlist_id=?1 ORDER BY position"
            };
            let mut statement = tx.prepare(sql)?;
            let params: &[&dyn rusqlite::ToSql] = if id == crate::favorites::FAVORITES_ID {
                &[]
            } else {
                &[&id]
            };
            for id in statement.query_map(params, |r| r.get::<_, i64>(0))? {
                let id = id?;
                if seen.insert(id) {
                    tracks.push(id);
                }
            }
        }
        tx.execute("INSERT INTO playlists(name) VALUES (?1)", [name])?;
        let id = tx.last_insert_rowid();
        for (position, track) in tracks.into_iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_tracks(playlist_id,track_id,position) VALUES (?1,?2,?3)",
                rusqlite::params![id, track, position as i64],
            )?;
        }
        tx.commit()?;
        // A separate copy keeps deletion of either source independent.
        if let Some(cover) = self
            .list_playlists()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.id == first || p.id == second)
            .find_map(|p| p.cover_path)
        {
            let _ = crate::appearance::AppearanceService::new(self.db.clone())
                .set_cover(id, &self.db.root().join(cover));
        }
        Ok(id)
    }

    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<()> {
        ensure!(
            id != crate::favorites::FAVORITES_ID,
            tr("Избранное — системный плейлист")
        );
        let name = name.trim();
        ensure!(!name.is_empty(), tr("Введите название плейлиста"));
        ensure!(
            name.chars().count() <= 100,
            tr("Название должно содержать не больше 100 символов")
        );
        self.repository.rename(id, name)
    }
    pub fn new(db: Database) -> Self {
        Self {
            repository: PlaylistRepository::new(db.clone()),
            db,
        }
    }
    pub fn create_playlist(&self, name: &str) -> Result<i64> {
        let name = name.trim();
        ensure!(!name.is_empty(), tr("Введите название плейлиста"));
        ensure!(
            name.chars().count() <= 100,
            tr("Название должно содержать не больше 100 символов")
        );
        self.repository.create(name)
    }
    pub fn list_playlists(&self) -> Result<Vec<Playlist>> {
        self.repository.list_playlists()
    }
    pub fn delete_playlist(&self, id: i64) -> Result<()> {
        ensure!(
            id != crate::favorites::FAVORITES_ID,
            tr("Избранное — системный плейлист")
        );
        let assets = self.repository.delete(id)?;
        // The transaction has committed before removing generated artwork; never touch audio.
        for relative in assets {
            let path = Path::new(&relative);
            if matches!(path.parent(), Some(parent) if parent == Path::new("artwork") || parent == Path::new("backgrounds"))
            {
                let _ = std::fs::remove_file(self.db.root().join(path));
            }
        }
        Ok(())
    }
    pub fn add_track(&self, playlist_id: i64, track_id: i64) -> Result<bool> {
        if playlist_id == crate::favorites::FAVORITES_ID {
            return crate::favorites::FavoritesService::new(self.db.clone()).set(track_id, true);
        }
        let added = self.repository.add_track(playlist_id, track_id)?;
        if added {
            if let Ok(track) = TrackRepository::new(self.db.clone()).get(track_id) {
                let _ = crate::appearance::AppearanceService::new(self.db.clone())
                    .set_cover_if_missing(
                        playlist_id,
                        &crate::artwork::track_path(self.db.root(), &track),
                    );
            }
        }
        Ok(added)
    }
    pub fn remove_track(&self, playlist_id: i64, track_id: i64) -> Result<bool> {
        if playlist_id == crate::favorites::FAVORITES_ID {
            return crate::favorites::FavoritesService::new(self.db.clone()).set(track_id, false);
        }
        self.repository.remove_track(playlist_id, track_id)
    }
    pub fn list_playlist_tracks(&self, playlist_id: i64) -> Result<Vec<Track>> {
        if playlist_id == crate::favorites::FAVORITES_ID {
            return crate::favorites::FavoritesService::new(self.db.clone()).tracks();
        }
        self.repository.list_playlist_tracks(playlist_id)
    }
}

pub fn application_directory() -> Result<PathBuf> {
    if let Some(path) = crate::app_identity::data_directory_override() {
        return Ok(PathBuf::from(path));
    }
    let config = crate::storage::config_directory()?;
    if let Some(path) = crate::storage::configured_directory(&config)? {
        return Ok(path);
    }
    // Keep using the existing library when upgrading from Lozex.
    if !crate::app_identity::IS_TEST
        && let Some(legacy) = directories::ProjectDirs::from("", "Lozex", "Lozex")
        && legacy.data_local_dir().join("library.sqlite3").is_file()
    {
        return Ok(legacy.data_local_dir().to_path_buf());
    }
    Ok(config)
}

#[cfg(test)]
mod tests;
