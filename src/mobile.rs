//! Platform-neutral Android command boundary. JNI owns no raw Rust pointers;
//! blocking work is dispatched by Android on IO workers, never the UI thread.
use crate::{
    appearance::{AppearanceService, parse_color},
    backgrounds::BackgroundService,
    db::Database,
    favorites::FavoritesService,
    preferences::Preferences,
    repositories::TrackRepository,
    services::{PlaylistService, TrackService},
    statistics::StatisticsService,
    sync::{DuplicateDecision, DuplicateTrack, SyncOptions, SyncSelection, SyncService},
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

#[derive(Default)]
struct Decisions {
    duplicates: Vec<DuplicateTrack>,
    reply: Option<mpsc::Sender<Vec<DuplicateDecision>>>,
}

pub struct MobileCore {
    db: Database,
    sync: Mutex<Option<SyncService>>,
    decisions: Mutex<Decisions>,
    cancelled: AtomicBool,
    settings: Mutex<()>,
}

impl MobileCore {
    pub fn open(root: &Path) -> Result<Self> {
        let db = Database::open(root)?;
        crate::i18n::set_language(Preferences::load(root).language);
        Ok(Self {
            db,
            sync: Mutex::new(None),
            decisions: Mutex::default(),
            cancelled: AtomicBool::new(false),
            settings: Mutex::new(()),
        })
    }

    fn sync(&self) -> Result<SyncService> {
        self.sync
            .lock()
            .unwrap()
            .clone()
            .context("Сначала включите синхронизацию / Enable sync first")
    }

    // Android copies SAF content into private staging before passing a path to Rust.
    // Resolve symlinks as well as '..', so the boundary cannot import arbitrary files.
    fn staged(&self, value: &Value) -> Result<PathBuf> {
        let path = Path::new(text(value, "path")?).canonicalize()?;
        ensure!(
            path.starts_with(self.db.root().join("staging").canonicalize()?),
            "File must be inside private staging"
        );
        Ok(path)
    }

    pub fn dispatch(&self, request: &str) -> String {
        let result = serde_json::from_str::<Value>(request)
            .map_err(anyhow::Error::from)
            .and_then(|v| self.command(&v));
        match result {
            Ok(value) => json!({"ok": true, "value": value}),
            Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
        }
        .to_string()
    }

    fn snapshot(&self) -> Result<Value> {
        let lists = PlaylistService::new(self.db.clone());
        let playlists = lists.list_playlists()?;
        let mut members = serde_json::Map::new();
        for p in &playlists {
            members.insert(
                p.id.to_string(),
                json!(
                    lists
                        .list_playlist_tracks(p.id)?
                        .iter()
                        .map(|t| t.id)
                        .collect::<Vec<_>>()
                ),
            );
        }
        Ok(json!({
            "tracks": TrackService::new(self.db.clone()).list_tracks()?,
            "playlists": playlists, "members": members,
            "favorites": FavoritesService::new(self.db.clone()).ids()?,
            "backgrounds": BackgroundService::new(self.db.clone()).list()?,
            "preferences": Preferences::load(self.db.root()),
            "statistics": StatisticsService::new(self.db.clone()).summary()?,
        }))
    }

    fn command(&self, v: &Value) -> Result<Value> {
        let tracks = TrackService::new(self.db.clone());
        let lists = PlaylistService::new(self.db.clone());
        let appearance = AppearanceService::new(self.db.clone());
        let backgrounds = BackgroundService::new(self.db.clone());
        match text(v, "op")? {
            "snapshot" => return self.snapshot(),
            "import" => {
                let path = self.staged(v)?;
                let track = if let Some(source) = v["source"].as_str() {
                    tracks.import_download(
                        &path,
                        source,
                        v["title"].as_str().map(str::to_owned),
                        v["artist"].as_str().map(str::to_owned),
                    )?
                } else {
                    tracks.import_local(&path)?
                };
                return Ok(json!(track));
            }
            "track_edit" => {
                tracks.update_metadata(id(v)?, text(v, "title")?, text(v, "artist")?)?;
            }
            "track_delete" => tracks.delete_track(id(v)?)?,
            "favorite" => {
                FavoritesService::new(self.db.clone())
                    .set(id(v)?, v["enabled"].as_bool().context("Missing enabled")?)?;
            }
            "playlist_create" => return Ok(json!(lists.create_playlist(text(v, "name")?)?)),
            "playlist_rename" => lists.rename_playlist(id(v)?, text(v, "name")?)?,
            "playlist_delete" => lists.delete_playlist(id(v)?)?,
            "playlist_add" => {
                lists.add_track(id(v)?, number(v, "track")?)?;
            }
            "playlist_remove" => {
                lists.remove_track(id(v)?, number(v, "track")?)?;
            }
            "cover_set" => appearance.set_cover(id(v)?, &self.staged(v)?)?,
            "cover_remove" => appearance.remove_cover(id(v)?)?,
            "header_color" => appearance.set_header_color(
                id(v)?,
                v["color"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(parse_color)
                    .transpose()?,
            )?,
            "background_set" => backgrounds.set_image(id(v)?, &self.staged(v)?)?,
            "background_remove" => backgrounds.remove(id(v)?)?,
            "background_blur" => backgrounds.set_blur(id(v)?, u8::try_from(number(v, "blur")?)?)?,
            "preferences" => {
                let _guard = self.settings.lock().unwrap();
                let mut p = serde_json::to_value(Preferences::load(self.db.root()))?;
                let patch = v["patch"]
                    .as_object()
                    .context("Missing preferences patch")?;
                // Keep the identity stable and do not persist Android service state.
                for (key, value) in patch {
                    ensure!(
                        matches!(
                            key.as_str(),
                            "language"
                                | "volume"
                                | "shuffle"
                                | "repeat"
                                | "theme"
                                | "sync_device_name"
                                | "sync_warn_duplicates"
                                | "sync_ignore_duplicates"
                        ),
                        "Unknown preference"
                    );
                    p[key] = value.clone();
                }
                let p: Preferences = serde_json::from_value(p)?;
                ensure!(
                    p.volume.is_finite() && (0.0..=1.0).contains(&p.volume),
                    "Invalid volume"
                );
                ensure!(
                    !p.sync_device_name.trim().is_empty()
                        && p.sync_device_name.chars().count() <= 100,
                    "Invalid device name"
                );
                p.save(self.db.root())?;
                crate::i18n::set_language(p.language);
                return Ok(json!(p));
            }
            "record" => {
                let ms = number(v, "ms")?;
                ensure!((0..=86_400_000).contains(&ms), "Invalid listening interval");
                StatisticsService::new(self.db.clone()).record(
                    text(v, "session")?,
                    id(v)?,
                    ms as u64,
                    number(v, "at")?,
                )?;
            }
            "metadata_cancel" => {
                self.cancelled.store(true, Ordering::Release);
            }
            "match" => {
                let candidates = crate::youtube::select_search_results(
                    text(v, "data")?.as_bytes(),
                    text(v, "title")?,
                    text(v, "artist")?,
                    v["duration_ms"].as_u64(),
                )?;
                return Ok(json!(
                    candidates
                        .iter()
                        .map(|url| url.as_str())
                        .collect::<Vec<_>>()
                ));
            }
            "metadata" => {
                self.cancelled.store(false, Ordering::Release);
                let url = crate::services::validate_music_url(text(v, "url")?)?;
                let collection = if url.host_str() == Some("open.spotify.com") {
                    crate::spotify::fetch(&url, &self.cancelled)?
                } else if url.host_str().is_some_and(crate::yandex::is_host) {
                    crate::yandex::fetch(&url, &self.cancelled)?
                } else {
                    return Ok(json!({"youtube": url.as_str()}));
                };
                return Ok(
                    json!({"title": collection.title, "collection": collection.is_collection,
                    "tracks": collection.tracks.iter().map(|t| json!({"title": t.title, "artist": t.artist, "url": t.url.as_str(), "duration_ms": t.duration_ms})).collect::<Vec<_>>() }),
                );
            }
            "sync_start" => {
                let mut slot = self.sync.lock().unwrap();
                if slot.is_none() {
                    let p = Preferences::load(self.db.root());
                    *slot = Some(SyncService::start(
                        self.db.clone(),
                        p.sync_device_id,
                        p.sync_device_name,
                    )?);
                }
            }
            "sync_stop" => {
                // Unblock duplicate confirmation before closing sockets.
                *self.decisions.lock().unwrap() = Decisions::default();
                if let Some(service) = self.sync.lock().unwrap().take() {
                    service.stop();
                }
            }
            "sync_pair" => self.sync()?.pair(text(v, "code")?)?,
            "sync_status" => {
                let slot = self.sync.lock().unwrap();
                let Some(service) = slot.as_ref() else {
                    return Ok(json!({"enabled": false}));
                };
                let status = service.status();
                return Ok(
                    json!({"enabled": service.is_enabled(), "transferring": service.is_transferring(),
                    "code": service.code(), "peer": status.peer, "error": status.error, "revision": status.revision,
                    "progress": status.progress.map(|p| json!({"title": p.title, "receiving": p.receiving, "bytes": p.bytes, "total": p.total, "complete": p.complete})),
                    "duplicates": self.decisions.lock().unwrap().duplicates }),
                );
            }
            "sync_decide" => {
                let mut pending = self.decisions.lock().unwrap();
                let all = v["copy"].as_bool();
                let decisions = v["choices"].as_array();
                ensure!(
                    all.is_some() || decisions.is_some(),
                    "Missing duplicate decisions"
                );
                let choices = pending
                    .duplicates
                    .iter()
                    .map(|t| DuplicateDecision {
                        index: t.index,
                        copy: all.unwrap_or_else(|| {
                            decisions.is_some_and(|items| {
                                items.iter().any(|d| {
                                    d["index"].as_u64() == Some(t.index as u64)
                                        && d["copy"].as_bool() == Some(true)
                                })
                            })
                        }),
                    })
                    .collect();
                pending
                    .reply
                    .take()
                    .context("No pending duplicates")?
                    .send(choices)?;
                pending.duplicates.clear();
            }
            "sync_send" => {
                let service = self.sync()?;
                let p = Preferences::load(self.db.root());
                let selection = match text(v, "selection")? {
                    "library" => SyncSelection::Library,
                    "playlist" => SyncSelection::Playlist(id(v)?),
                    "track" => {
                        TrackRepository::new(self.db.clone()).get(id(v)?)?;
                        SyncSelection::Track(id(v)?)
                    }
                    _ => bail!("Unknown selection"),
                };
                let report = service.send_selection(
                    selection,
                    SyncOptions {
                        warn_duplicates: p.sync_warn_duplicates,
                        ignore_duplicates: p.sync_ignore_duplicates,
                    },
                    |duplicates| {
                        let (send, receive) = mpsc::channel();
                        *self.decisions.lock().unwrap() = Decisions {
                            duplicates: duplicates.to_vec(),
                            reply: Some(send),
                        };
                        let result = receive
                            .recv_timeout(Duration::from_secs(120))
                            .context("Duplicate confirmation cancelled or timed out");
                        *self.decisions.lock().unwrap() = Decisions::default();
                        result
                    },
                )?;
                return Ok(json!(report));
            }
            _ => bail!("Unknown mobile command"),
        }
        Ok(Value::Null)
    }
}

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key].as_str().with_context(|| format!("Missing {key}"))
}
fn number(v: &Value, key: &str) -> Result<i64> {
    v[key].as_i64().with_context(|| format!("Missing {key}"))
}
fn id(v: &Value) -> Result<i64> {
    number(v, "id")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(core: &MobileCore, value: Value) -> Value {
        let result: Value = serde_json::from_str(&core.dispatch(&value.to_string())).unwrap();
        assert_eq!(result["ok"], true, "{result}");
        result["value"].clone()
    }
    #[test]
    fn bridge_preserves_desktop_library_and_settings() {
        let root = tempfile::tempdir().unwrap();
        let core = MobileCore::open(root.path()).unwrap();
        let id = call(&core, json!({"op":"playlist_create","name":"Android"}));
        call(
            &core,
            json!({"op":"header_color","id":id,"color":"#123456"}),
        );
        call(
            &core,
            json!({"op":"preferences","patch":{"theme":"cosmic","volume":0.3}}),
        );
        let snapshot = call(&core, json!({"op":"snapshot"}));
        assert_eq!(snapshot["playlists"][0]["header_color"], 0x123456);
        assert_eq!(snapshot["preferences"]["theme"], "cosmic");
        let db = Database::open(root.path()).unwrap();
        assert_eq!(
            PlaylistService::new(db).list_playlists().unwrap()[0].name,
            "Android"
        );
        let reopened = MobileCore::open(root.path()).unwrap();
        assert_eq!(
            call(&reopened, json!({"op":"snapshot"}))["preferences"]["volume"],
            json!(0.3_f32)
        );
    }
    #[test]
    fn invalid_commands_paths_and_preferences_return_errors() {
        let root = tempfile::tempdir().unwrap();
        let core = MobileCore::open(root.path()).unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        for request in [
            json!({"op":"import","path":outside.path()}),
            json!({"op":"preferences","patch":{"volume":2}}),
            json!({"op":"playlist_delete","id":-1}),
            json!({"op":"sync_send","selection":"library"}),
        ] {
            let result: Value = serde_json::from_str(&core.dispatch(&request.to_string())).unwrap();
            assert_eq!(result["ok"], false);
        }
        assert!(core.dispatch("{").contains("\"ok\":false"));
        assert!(
            call(&core, json!({"op":"snapshot"}))["tracks"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
