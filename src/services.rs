use crate::i18n::tr;
use crate::localized_format;
use crate::youtube::{
    DownloadInfo, YoutubeClient, YoutubeDownloadReport, YoutubeEntry, YoutubeFailure,
    YoutubePlaylist, YoutubeProgress, YoutubeSource, playlist_url,
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

#[derive(Clone)]
pub struct TrackService {
    db: Database,
    repository: TrackRepository,
}

impl TrackService {
    pub fn download_collection(
        &self,
        input: &str,
        cancel: &AtomicBool,
        mut progress: impl FnMut(YoutubeProgress),
    ) -> Result<YoutubeDownloadReport> {
        let url = validate_music_url(input)?;
        if url.host_str() != Some("open.spotify.com") {
            return self.download_youtube_collection(input, cancel, progress);
        }
        progress(YoutubeProgress {
            completed: 0,
            total: 0,
            current_title: None,
        });
        let collection = match crate::spotify::fetch(&url, cancel) {
            Err(_) if cancel.load(Ordering::Relaxed) => {
                return Ok(YoutubeDownloadReport {
                    cancelled: true,
                    ..Default::default()
                });
            }
            result => result?,
        };
        let source = crate::spotify::SpotifySource {
            youtube: YoutubeClient::new(self.db.root())?,
            collection,
        };
        self.download_entries(
            source.entries(),
            source.collection.is_collection,
            &source,
            cancel,
            progress,
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
        // Parse before copying: a failed import must not leave a library entry or an orphan file.
        let tagged = Probe::open(source)?
            .read()
            .context(tr("Не удалось прочитать аудиофайл"))?;
        let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
        let info = info.unwrap_or_default();
        let title = info
            .track
            .or_else(|| {
                tag.and_then(|t| t.title())
                    .map(|s| s.into_owned())
                    .filter(|s| !s.trim().is_empty())
            })
            .or(info.title)
            .unwrap_or_else(|| {
                source
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let artist = info
            .artist
            .or_else(|| {
                tag.and_then(|t| t.artist())
                    .map(|s| s.into_owned())
                    .filter(|s| !s.trim().is_empty())
            })
            .or(info.uploader)
            .unwrap_or_default();
        let duration = tagged.properties().duration().as_millis();
        let duration_ms = if duration > 0 {
            Some(i64::try_from(duration)?)
        } else {
            info.duration
                .filter(|v| v.is_finite() && *v >= 0.0)
                .map(|v| (v * 1000.0) as i64)
        };
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
        mut progress: impl FnMut(YoutubeProgress),
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
                entries: vec![YoutubeEntry {
                    title: tr("Трек YouTube").into(),
                    url: Some(url.clone()),
                }],
            }
        };
        self.download_entries(playlist, playlist_link.is_some(), source, cancel, progress)
    }

    fn download_entries(
        &self,
        playlist: YoutubePlaylist,
        is_playlist: bool,
        source: &impl YoutubeSource,
        cancel: &AtomicBool,
        mut progress: impl FnMut(YoutubeProgress),
    ) -> Result<YoutubeDownloadReport> {
        let mut report = YoutubeDownloadReport {
            total: playlist.entries.len(),
            ..Default::default()
        };
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            return Ok(report);
        }
        let playlists = PlaylistService::new(self.db.clone());
        if is_playlist {
            let name: String = playlist.title.trim().chars().take(100).collect();
            report.playlist_id = Some(playlists.create_playlist(if name.is_empty() {
                tr("Плейлист YouTube")
            } else {
                &name
            })?);
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
            let result = (|| -> Result<Track> {
                let url = entry.url.context(tr(
                    "YouTube не предоставил ссылку на этот трек (видео удалено или скрыто)",
                ))?;
                let staging = tempfile::tempdir_in(self.db.root().join("staging"))?;
                let audio = source.track(&url, staging.path(), cancel)?;
                ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
                self.import_file(&audio.path, Some(url.to_string()), Some(audio.info))
            })();
            match result {
                Ok(track) => {
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
                    report.tracks.push(track);
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
}

pub fn validate_music_url(input: &str) -> Result<url::Url> {
    if url::Url::parse(input.trim())
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .as_deref()
        == Some("open.spotify.com")
    {
        crate::spotify::validate_spotify_url(input)
    } else {
        validate_youtube_url(input).context(tr(
            "Вставьте ссылку YouTube или Spotify на трек или открытый плейлист",
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
    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<()> {
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
        self.repository.add_track(playlist_id, track_id)
    }
    pub fn remove_track(&self, playlist_id: i64, track_id: i64) -> Result<bool> {
        self.repository.remove_track(playlist_id, track_id)
    }
    pub fn list_playlist_tracks(&self, playlist_id: i64) -> Result<Vec<Track>> {
        self.repository.list_playlist_tracks(playlist_id)
    }
}

pub fn application_directory() -> Result<PathBuf> {
    if let Some(path) =
        std::env::var_os("SHARD_DATA_DIR").or_else(|| std::env::var_os("LOZEX_DATA_DIR"))
    {
        return Ok(PathBuf::from(path));
    }
    // Keep using the existing library when upgrading from Lozex.
    if let Some(legacy) = directories::ProjectDirs::from("", "Lozex", "Lozex")
        && legacy.data_local_dir().join("library.sqlite3").is_file()
    {
        return Ok(legacy.data_local_dir().to_path_buf());
    }
    directories::ProjectDirs::from("", "Shard", "Shard")
        .map(|dirs| dirs.data_local_dir().to_path_buf())
        .context(tr("Не удалось определить папку данных приложения"))
}

#[cfg(test)]
mod tests;
