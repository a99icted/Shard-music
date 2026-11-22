//! Copy a quiescent library to a new location, then atomically switch the startup pointer.
//! The source is retained as a backup; existing destinations are never overwritten.
use crate::{db::Database, localized_format, preferences::Preferences};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn config_directory() -> Result<PathBuf> {
    directories::ProjectDirs::from("", "Shard", "Shard")
        .map(|dirs| dirs.data_local_dir().to_path_buf())
        .context("Could not locate SHARD settings")
}

pub fn configured_directory(config: &Path) -> Result<Option<PathBuf>> {
    let location = config.join("library-location.json");
    if !location.exists() {
        return Ok(None);
    }
    let path: PathBuf = serde_json::from_slice(&fs::read(location)?)?;
    ensure!(
        path.is_absolute() && path.join("library.sqlite3").is_file(),
        "{}",
        localized_format!(
            "Хранилище недоступно: {}. Подключите диск и запустите SHARD снова.",
            "Storage is unavailable: {}. Connect the drive and restart SHARD.",
            path.display()
        )
    );
    Ok(Some(path))
}

/// Stop all library writers and playback before calling. Playlist appearance and
/// preferences are retained. Files are staged so a failed SQL transaction can roll back.
pub fn clear_music(db: &Database) -> Result<usize> {
    use crate::repositories::TrackRepository;
    let root = db.root().canonicalize()?;
    let tracks = TrackRepository::new(db.clone()).list_tracks()?;
    let staged = tempfile::Builder::new()
        .prefix("clear-music-")
        .tempdir_in(root.join("staging"))?;
    let mut paths = vec![root.join("audio"), root.join("sync-cache")];
    paths.extend(tracks.iter().map(|t| crate::artwork::track_path(&root, t)));
    paths.extend(
        tracks
            .iter()
            .map(|t| crate::artwork::thumbnail_path(&crate::artwork::track_path(&root, t))),
    );
    paths.sort();
    paths.dedup();
    let mut moved = Vec::new();
    let result = (|| -> Result<()> {
        for (index, path) in paths.into_iter().enumerate() {
            if !path.try_exists()? {
                continue;
            }
            // Never follow a linked folder into files outside this library.
            ensure!(
                !fs::symlink_metadata(&path)?.file_type().is_symlink(),
                "Linked music files cannot be cleared"
            );
            let target = staged.path().join(index.to_string());
            fs::rename(&path, &target)?;
            moved.push((path, target));
        }
        fs::create_dir_all(root.join("audio"))?;
        let mut connection = db.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM tracks", [])?;
        // Old acknowledgements must not make a future reimport appear complete.
        transaction.execute("DELETE FROM sync_receipts", [])?;
        transaction.execute("DELETE FROM sync_outgoing", [])?;
        transaction.commit()?;
        Ok(())
    })();
    if let Err(error) = result {
        let mut rollback_error = None;
        for (path, target) in moved.into_iter().rev() {
            if path == root.join("audio") && path.is_dir() {
                let _ = fs::remove_dir(&path); // Empty replacement only.
            }
            if let Err(failure) = fs::rename(&target, &path) {
                rollback_error = Some(failure);
            }
        }
        if let Some(failure) = rollback_error {
            let recovery = staged.keep();
            return Err(error).context(format!(
                "Could not restore files: {failure}. Recovery folder: {}",
                recovery.display()
            ));
        }
        return Err(error);
    }
    staged
        .close()
        .context("Music entries cleared, but some staged files could not be removed")?;
    Ok(tracks.len())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    ensure!(
        !fs::symlink_metadata(source)?.file_type().is_symlink(),
        "Linked storage folders cannot be moved"
    );
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(!kind.is_symlink(), "Linked storage files cannot be moved");
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            ensure!(kind.is_file(), "Unsupported storage file");
            fs::copy(entry.path(), &target)?;
            fs::OpenOptions::new()
                .write(true)
                .open(target)?
                .sync_all()?;
        }
    }
    Ok(())
}

/// Call after stopping downloads, synchronization and playback, and flushing preferences.
pub fn relocate_library(
    source: &Path,
    destination: &Path,
    config: &Path,
    preferences: &Preferences,
) -> Result<PathBuf> {
    let source = source.canonicalize()?;
    let parent = destination
        .parent()
        .context("Choose a storage folder")?
        .canonicalize()?;
    let destination = parent.join(destination.file_name().context("Choose a storage folder")?);
    ensure!(
        !destination.exists(),
        "{}",
        localized_format!(
            "Папка {} уже существует. Выберите другую папку для переноса.",
            "Folder {} already exists. Choose a different destination.",
            destination.display()
        )
    );
    ensure!(
        !parent.starts_with(&source) && !source.starts_with(&destination),
        "{}",
        localized_format!(
            "Выберите папку вне текущего хранилища",
            "Choose a folder outside the current storage"
        )
    );
    let staged = tempfile::Builder::new()
        .prefix(".shard-transfer-")
        .tempdir_in(&parent)?;
    for name in ["audio", "artwork", "backgrounds", "sync-cache", "tools"] {
        let path = source.join(name);
        if path.exists() {
            copy_directory(&path, &staged.path().join(name))?;
        }
    }
    let db = Database::open(&source)?;
    let snapshot = staged.path().join("library.sqlite3");
    db.connect()?
        .execute("VACUUM INTO ?1", [snapshot.to_string_lossy().as_ref()])?;
    fs::OpenOptions::new()
        .write(true)
        .open(&snapshot)?
        .sync_all()?;
    preferences.save(staged.path())?;
    let copied = Database::open(staged.path())?;
    let check: String = copied
        .connect()?
        .query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    ensure!(check == "ok", "Library verification failed: {check}");
    drop(copied);
    fs::rename(staged.path(), &destination)?;
    // Write the pointer only after a complete, verified copy exists.
    fs::create_dir_all(config)?;
    let mut location = tempfile::NamedTempFile::new_in(config)?;
    serde_json::to_writer(location.as_file_mut(), &destination)?;
    location.as_file().sync_all()?;
    location.persist(config.join("library-location.json"))?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{models::NewTrack, repositories::TrackRepository, services::PlaylistService};
    #[test]
    fn clearing_music_keeps_playlists_and_settings_and_rolls_back_on_sql_failure() {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(root.path()).unwrap();
        let tracks = TrackRepository::new(db.clone());
        let track = tracks
            .insert(&NewTrack {
                title: "Keep".into(),
                artist: "Artist".into(),
                source_url: None,
                file_path: "audio/one.wav".into(),
                duration_ms: Some(1000),
            })
            .unwrap();
        fs::write(root.path().join("audio/one.wav"), b"audio").unwrap();
        fs::write(root.path().join("audio/orphan.wav"), b"orphan").unwrap();
        fs::create_dir_all(root.path().join("artwork/thumbnails")).unwrap();
        for path in [
            "artwork/one.png",
            "artwork/thumbnails/one.png",
            "artwork/playlist.png",
        ] {
            fs::write(root.path().join(path), b"cover").unwrap();
        }
        fs::create_dir(root.path().join("sync-cache")).unwrap();
        fs::write(root.path().join("sync-cache/partial"), b"unfinished").unwrap();
        let playlists = PlaylistService::new(db.clone());
        let id = playlists.create_playlist("Playlist").unwrap();
        playlists.add_track(id, track.id).unwrap();
        crate::favorites::FavoritesService::new(db.clone())
            .set(track.id, true)
            .unwrap();
        Preferences::default().save(root.path()).unwrap();
        crate::statistics::StatisticsService::new(db.clone())
            .record("test", track.id, 500, 1_700_000_000)
            .unwrap();
        db.connect().unwrap().execute_batch("CREATE TRIGGER reject_clear BEFORE DELETE ON tracks BEGIN SELECT RAISE(ABORT,'blocked'); END;").unwrap();
        assert!(clear_music(&db).is_err());
        assert_eq!(
            fs::read(root.path().join("audio/one.wav")).unwrap(),
            b"audio"
        );
        assert!(root.path().join("artwork/thumbnails/one.png").is_file());
        assert_eq!(tracks.list_tracks().unwrap().len(), 1);
        db.connect()
            .unwrap()
            .execute_batch("DROP TRIGGER reject_clear;")
            .unwrap();
        assert_eq!(clear_music(&db).unwrap(), 1);
        assert!(tracks.list_tracks().unwrap().is_empty());
        assert_eq!(fs::read_dir(root.path().join("audio")).unwrap().count(), 0);
        assert!(!root.path().join("sync-cache").exists());
        assert!(!root.path().join("artwork/one.png").exists());
        assert!(!root.path().join("artwork/thumbnails/one.png").exists());
        assert!(root.path().join("artwork/playlist.png").is_file());
        assert!(root.path().join("settings.json").is_file());
        assert_eq!(playlists.list_playlists().unwrap().len(), 1);
        assert!(playlists.list_playlist_tracks(id).unwrap().is_empty());
        assert!(
            crate::favorites::FavoritesService::new(db.clone())
                .ids()
                .unwrap()
                .is_empty()
        );
        let total: i64 = db
            .connect()
            .unwrap()
            .query_row("SELECT SUM(listened_ms) FROM listening_sessions", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(total, 500);
        assert_eq!(clear_music(&db).unwrap(), 0);
    }
    #[test]
    fn failed_copy_does_not_change_the_location_or_touch_existing_files() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let config = root.path().join("config");
        Database::open(&source).unwrap();
        fs::write(source.join("audio/keep.wav"), b"keep").unwrap();
        let existing = root.path().join("existing");
        fs::create_dir(&existing).unwrap();
        fs::write(existing.join("keep"), b"untouched").unwrap();
        assert!(relocate_library(&source, &existing, &config, &Preferences::default()).is_err());
        assert_eq!(fs::read(existing.join("keep")).unwrap(), b"untouched");
        fs::write(source.join("library.sqlite3"), b"invalid database").unwrap();
        let destination = root.path().join("new");
        assert!(relocate_library(&source, &destination, &config, &Preferences::default()).is_err());
        assert!(!destination.exists());
        assert!(!config.join("library-location.json").exists());
        assert_eq!(fs::read(source.join("audio/keep.wav")).unwrap(), b"keep");
    }
    #[test]
    fn relocation_keeps_audio_playlists_settings_and_a_source_backup() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let config = root.path().join("config");
        let destination = root.path().join("destination");
        let db = Database::open(&source).unwrap();
        fs::write(source.join("audio/test.wav"), b"audio fixture").unwrap();
        fs::create_dir(source.join("artwork")).unwrap();
        fs::write(source.join("artwork/test.png"), b"cover fixture").unwrap();
        let track = TrackRepository::new(db.clone())
            .insert(&NewTrack {
                title: "Track".into(),
                artist: "Artist".into(),
                source_url: None,
                file_path: "audio/test.wav".into(),
                duration_ms: Some(1000),
            })
            .unwrap();
        let lists = PlaylistService::new(db);
        let id = lists.create_playlist("Playlist").unwrap();
        lists.add_track(id, track.id).unwrap();
        let preferences = Preferences {
            volume: 0.42,
            ..Default::default()
        };
        let moved = relocate_library(&source, &destination, &config, &preferences).unwrap();
        assert_eq!(configured_directory(&config).unwrap().unwrap(), moved);
        assert_eq!(
            fs::read(moved.join("audio/test.wav")).unwrap(),
            b"audio fixture"
        );
        assert_eq!(
            fs::read(moved.join("artwork/test.png")).unwrap(),
            b"cover fixture"
        );
        assert_eq!(Preferences::load(&moved).volume, 0.42);
        assert_eq!(
            PlaylistService::new(Database::open(&moved).unwrap())
                .list_playlist_tracks(id)
                .unwrap(),
            vec![track]
        );
        assert!(source.join("audio/test.wav").is_file());
        assert!(source.join("library.sqlite3").is_file());
        assert!(relocate_library(&source, &destination, &config, &preferences).is_err());
        assert!(relocate_library(&source, &source.join("nested"), &config, &preferences).is_err());
        // A later move replaces the existing startup pointer and keeps the first copy.
        let final_path = root.path().join("second-destination");
        let final_path = relocate_library(&moved, &final_path, &config, &preferences).unwrap();
        assert_eq!(configured_directory(&config).unwrap().unwrap(), final_path);
        assert!(moved.join("audio/test.wav").is_file());
    }
    #[test]
    fn unavailable_configured_storage_is_an_error_instead_of_an_empty_library() {
        let config = tempfile::tempdir().unwrap();
        assert!(configured_directory(config.path()).unwrap().is_none());
        fs::write(
            config.path().join("library-location.json"),
            serde_json::to_vec(&config.path().join("unplugged")).unwrap(),
        )
        .unwrap();
        assert!(configured_directory(config.path()).is_err());
    }
}
