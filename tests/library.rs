use shard::{
    db::Database,
    models::NewTrack,
    repositories::{PlaylistRepository, TrackRepository},
    services::{PlaylistService, TrackService, validate_youtube_url},
};
use std::{fs, io::Write, path::Path};

fn wav(path: &Path) {
    let samples = 8_000u32;
    let data_size = samples * 2;
    let mut file = fs::File::create(path).unwrap();
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + data_size).to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&8_000u32.to_le_bytes()).unwrap();
    file.write_all(&16_000u32.to_le_bytes()).unwrap();
    file.write_all(&2u16.to_le_bytes()).unwrap();
    file.write_all(&16u16.to_le_bytes()).unwrap();
    file.write_all(b"data").unwrap();
    file.write_all(&data_size.to_le_bytes()).unwrap();
    file.write_all(&vec![0; data_size as usize]).unwrap();
}

#[test]
fn metadata_edits_survive_restart_and_preserve_audio_and_membership() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("library");
    let source = dir.path().join("original.wav");
    wav(&source);
    let db = Database::open(&root).unwrap();
    let tracks = TrackService::new(db.clone());
    let lists = PlaylistService::new(db.clone());
    let track = tracks.import_local(&source).unwrap();
    let playlist = lists.create_playlist("Before").unwrap();
    lists.add_track(playlist, track.id).unwrap();
    let bytes = fs::read(tracks.audio_path(&track).unwrap()).unwrap();
    assert!(tracks.update_metadata(track.id, "  ", "Artist").is_err());
    assert!(
        tracks
            .update_metadata(track.id, &"a".repeat(501), "Artist")
            .is_err()
    );
    assert!(lists.rename_playlist(playlist, " ").is_err());
    assert!(lists.rename_playlist(playlist, &"a".repeat(101)).is_err());
    assert!(lists.rename_playlist(-1, "Missing").is_err());
    assert!(tracks.update_metadata(-1, "Missing", "").is_err());
    tracks
        .update_metadata(track.id, "  Новое название 🎵  ", "  New artist  ")
        .unwrap();
    lists.rename_playlist(playlist, "  New name  ").unwrap();
    let reopened = Database::open(&root).unwrap();
    let actual = PlaylistService::new(reopened.clone())
        .list_playlist_tracks(playlist)
        .unwrap();
    assert_eq!(actual.len(), 1);
    assert_eq!(actual[0].id, track.id);
    assert_eq!(actual[0].title, "Новое название 🎵");
    assert_eq!(actual[0].artist, "New artist");
    assert_eq!(actual[0].file_path, track.file_path);
    assert_eq!(actual[0].duration_ms, track.duration_ms);
    assert_eq!(
        PlaylistService::new(reopened).list_playlists().unwrap()[0].name,
        "New name"
    );
    assert_eq!(fs::read(tracks.audio_path(&track).unwrap()).unwrap(), bytes);
}

#[test]
fn imported_audio_and_playlist_survive_restart_in_insertion_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("app");
    let source = dir.path().join("Первый трек.wav");
    wav(&source);
    let db = Database::open(&root).unwrap();
    let tracks = TrackService::new(db.clone());
    let first = tracks.import_local(&source).unwrap();
    let second = tracks.import_local(&source).unwrap();
    let third = tracks.import_local(&source).unwrap();
    assert_eq!(first.title, "Первый трек");
    assert_eq!(first.duration_ms, Some(1000));
    assert!(!Path::new(&first.file_path).is_absolute());
    assert_eq!(
        fs::read(tracks.audio_path(&first).unwrap()).unwrap(),
        fs::read(&source).unwrap()
    );
    let lists = PlaylistService::new(db);
    let playlist = lists.create_playlist("  Дорога  ").unwrap();
    let another = lists.create_playlist("Работа").unwrap();
    assert!(lists.add_track(playlist, second.id).unwrap());
    assert!(lists.add_track(playlist, first.id).unwrap());
    assert!(!lists.add_track(playlist, first.id).unwrap());
    assert!(lists.add_track(another, first.id).unwrap());
    drop(lists);
    drop(tracks);
    fs::remove_file(source).unwrap(); // playback must not depend on the original file
    let db = Database::open(&root).unwrap();
    let tracks = TrackService::new(db.clone());
    let lists = PlaylistService::new(db);
    assert_eq!(tracks.list_tracks().unwrap().len(), 3);
    let actual = lists.list_playlist_tracks(playlist).unwrap();
    assert_eq!(
        actual.iter().map(|t| t.id).collect::<Vec<_>>(),
        [second.id, first.id]
    );
    assert!(!actual.iter().any(|t| t.id == third.id));
    assert!(tracks.audio_path(&first).unwrap().is_file());
    let all = lists.list_playlists().unwrap();
    assert_eq!(all[0].name, "Дорога");
    assert_eq!(all[0].track_count, 2);
    assert_eq!(all[0].duration_ms, 2000);
    assert_eq!(all[1].duration_ms, 1000);
    assert_eq!(lists.list_playlist_tracks(another).unwrap().len(), 1);
}

#[test]
fn schema_enforces_foreign_keys_positions_uniqueness_and_cascades() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    let tracks = TrackRepository::new(db.clone());
    let entry = NewTrack {
        title: "test".into(),
        artist: "".into(),
        source_url: None,
        file_path: "audio/test.wav".into(),
        duration_ms: None,
    };
    let track = tracks.insert(&entry).unwrap();
    assert!(tracks.insert(&entry).is_err());
    let playlists = PlaylistRepository::new(db.clone());
    let id = playlists.create("Test").unwrap();
    assert!(playlists.add_track(id, 999).is_err());
    assert!(playlists.add_track(999, track.id).is_err());
    playlists.add_track(id, track.id).unwrap();
    let conn = db.connect().unwrap();
    assert!(
        conn.execute(
            "INSERT INTO playlist_tracks VALUES (?1, ?2, -1)",
            [id, track.id]
        )
        .is_err()
    );
    let other = tracks
        .insert(&NewTrack {
            file_path: "audio/other.wav".into(),
            ..entry
        })
        .unwrap();
    assert!(
        conn.execute(
            "INSERT INTO playlist_tracks VALUES (?1, ?2, 0)",
            [id, other.id]
        )
        .is_err()
    );
    let flag: i64 = conn
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .unwrap();
    assert_eq!(flag, 1);
    conn.execute("DELETE FROM tracks WHERE id=?1", [track.id])
        .unwrap();
    assert!(playlists.list_playlist_tracks(id).unwrap().is_empty());
    playlists.add_track(id, other.id).unwrap();
    conn.execute("DELETE FROM playlists WHERE id=?1", [id])
        .unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM playlist_tracks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn invalid_import_does_not_leave_files_or_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path().join("app")).unwrap();
    let service = TrackService::new(db.clone());
    let invalid = dir.path().join("broken.mp3");
    fs::write(&invalid, b"not audio").unwrap();
    assert!(service.import_local(&invalid).is_err());
    assert!(
        service
            .import_local(&dir.path().join("missing.wav"))
            .is_err()
    );
    assert!(service.list_tracks().unwrap().is_empty());
    assert_eq!(fs::read_dir(db.root().join("audio")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(db.root().join("staging")).unwrap().count(), 0);
}

#[test]
fn failed_database_write_removes_the_imported_copy() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path().join("app")).unwrap();
    db.connect().unwrap().execute_batch("CREATE TRIGGER reject_import BEFORE INSERT ON tracks BEGIN SELECT RAISE(ABORT, 'simulated disk/database failure'); END;").unwrap();
    let source = dir.path().join("valid.wav");
    wav(&source);
    let service = TrackService::new(db.clone());
    assert!(service.import_local(&source).is_err());
    assert!(source.exists());
    assert!(service.list_tracks().unwrap().is_empty());
    assert_eq!(fs::read_dir(db.root().join("audio")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(db.root().join("staging")).unwrap().count(), 0);
}

#[test]
fn validates_names_and_youtube_urls() {
    let dir = tempfile::tempdir().unwrap();
    let service = PlaylistService::new(Database::open(dir.path()).unwrap());
    assert!(service.create_playlist("  ").is_err());
    assert!(service.create_playlist(&"я".repeat(101)).is_err());
    for url in [
        "https://youtu.be/abc",
        "https://www.youtube.com/watch?v=abc",
        "https://music.youtube.com/watch?v=abc",
    ] {
        assert!(validate_youtube_url(url).is_ok());
    }
    for url in [
        "file:///etc/passwd",
        "http://youtu.be/abc",
        "https://youtube.com.evil.test/watch?v=x",
        "https://evil.test",
        "--exec=calc",
        "https://youtube.com/",
        "https://user@youtube.com/watch?v=x",
    ] {
        assert!(validate_youtube_url(url).is_err());
    }
}

#[test]
fn concurrent_appends_keep_a_unique_contiguous_order() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).unwrap();
    let tracks = TrackRepository::new(db.clone());
    let playlists = PlaylistRepository::new(db.clone());
    let id = playlists.create("Parallel").unwrap();
    let handles: Vec<_> = (0..8)
        .map(|n| {
            let track = tracks
                .insert(&NewTrack {
                    title: format!("{n}"),
                    artist: String::new(),
                    source_url: None,
                    file_path: format!("audio/{n}.wav"),
                    duration_ms: None,
                })
                .unwrap();
            let repository = playlists.clone();
            std::thread::spawn(move || repository.add_track(id, track.id).unwrap())
        })
        .collect();
    for handle in handles {
        assert!(handle.join().unwrap());
    }
    let conn = db.connect().unwrap();
    let mut query = conn
        .prepare("SELECT position FROM playlist_tracks ORDER BY position")
        .unwrap();
    let positions = query
        .query_map([], |r| r.get::<_, i64>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(positions, (0..8).collect::<Vec<_>>());
}

#[test]
fn removing_from_one_playlist_preserves_audio_and_compacts_positions() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(root.path().join("app")).unwrap();
    let source = root.path().join("original.wav");
    wav(&source);
    let tracks = TrackService::new(db.clone());
    let entries: Vec<_> = (0..4)
        .map(|_| tracks.import_local(&source).unwrap())
        .collect();
    let lists = PlaylistService::new(db.clone());
    let first = lists.create_playlist("First").unwrap();
    let second = lists.create_playlist("Second").unwrap();
    for track in &entries {
        lists.add_track(first, track.id).unwrap();
    }
    lists.add_track(second, entries[1].id).unwrap();
    assert!(lists.remove_track(first, entries[1].id).unwrap());
    assert!(!lists.remove_track(first, entries[1].id).unwrap());
    assert_eq!(
        lists
            .list_playlist_tracks(first)
            .unwrap()
            .iter()
            .map(|track| track.id)
            .collect::<Vec<_>>(),
        [entries[0].id, entries[2].id, entries[3].id]
    );
    assert_eq!(
        lists.list_playlist_tracks(second).unwrap()[0].id,
        entries[1].id
    );
    assert!(tracks.audio_path(&entries[1]).unwrap().is_file());
    let conn = db.connect().unwrap();
    let mut stmt = conn
        .prepare("SELECT position FROM playlist_tracks WHERE playlist_id=?1 ORDER BY position")
        .unwrap();
    assert_eq!(
        stmt.query_map([first], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap(),
        [0, 1, 2]
    );
    lists.add_track(first, entries[1].id).unwrap();
    assert_eq!(
        lists
            .list_playlist_tracks(first)
            .unwrap()
            .last()
            .unwrap()
            .id,
        entries[1].id
    );
}

#[test]
fn deleting_from_storage_removes_audio_and_all_memberships_but_preserves_original() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(root.path().join("app")).unwrap();
    let source = root.path().join("original.wav");
    wav(&source);
    let tracks = TrackService::new(db.clone());
    let deleted = tracks.import_local(&source).unwrap();
    let keep = tracks.import_local(&source).unwrap();
    let lists = PlaylistService::new(db.clone());
    let first = lists.create_playlist("First").unwrap();
    let second = lists.create_playlist("Second").unwrap();
    for list in [first, second] {
        lists.add_track(list, deleted.id).unwrap();
        lists.add_track(list, keep.id).unwrap();
    }
    tracks.delete_track(deleted.id).unwrap();
    assert!(source.is_file());
    assert!(!tracks.audio_path(&deleted).unwrap().exists());
    assert_eq!(
        tracks.list_tracks().unwrap().as_slice(),
        std::slice::from_ref(&keep)
    );
    for list in [first, second] {
        assert_eq!(
            lists.list_playlist_tracks(list).unwrap().as_slice(),
            std::slice::from_ref(&keep)
        );
    }
    assert_eq!(fs::read_dir(db.root().join("staging")).unwrap().count(), 0);
    fs::remove_file(tracks.audio_path(&keep).unwrap()).unwrap();
    tracks.delete_track(keep.id).unwrap();
    assert!(tracks.list_tracks().unwrap().is_empty());
}

#[test]
fn deleting_rolls_back_file_move_when_sqlite_refuses_the_change() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(root.path().join("app")).unwrap();
    let source = root.path().join("original.wav");
    wav(&source);
    let tracks = TrackService::new(db.clone());
    let track = tracks.import_local(&source).unwrap();
    db.connect().unwrap().execute_batch("CREATE TRIGGER reject_delete BEFORE DELETE ON tracks BEGIN SELECT RAISE(ABORT, 'simulated failure'); END;").unwrap();
    assert!(tracks.delete_track(track.id).is_err());
    assert!(tracks.audio_path(&track).unwrap().is_file());
    assert_eq!(tracks.list_tracks().unwrap(), [track]);
    assert_eq!(fs::read_dir(db.root().join("staging")).unwrap().count(), 0);
}

#[test]
fn deleting_playlist_preserves_music_and_other_playlists_after_restart() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(root.path().join("app")).unwrap();
    let source = root.path().join("original.wav");
    wav(&source);
    let tracks = TrackService::new(db.clone());
    let track = tracks.import_local(&source).unwrap();
    let lists = PlaylistService::new(db.clone());
    let deleted = lists.create_playlist("Delete me").unwrap();
    let keep = lists.create_playlist("Keep me").unwrap();
    for id in [deleted, keep] {
        lists.add_track(id, track.id).unwrap();
    }
    let image = root.path().join("cover.png");
    image::RgbaImage::from_pixel(16, 16, image::Rgba([80, 40, 120, 255]))
        .save(&image)
        .unwrap();
    shard::appearance::AppearanceService::new(db.clone())
        .set_cover(deleted, &image)
        .unwrap();
    let backgrounds = shard::backgrounds::BackgroundService::new(db.clone());
    backgrounds.set_image(deleted, &image).unwrap();
    backgrounds.set_image(0, &image).unwrap();
    let cover = lists.list_playlists().unwrap()[0]
        .cover_path
        .clone()
        .unwrap();
    let old_background = backgrounds
        .list()
        .unwrap()
        .into_iter()
        .find(|b| b.scope == deleted)
        .unwrap();
    let global_background = backgrounds
        .list()
        .unwrap()
        .into_iter()
        .find(|b| b.scope == 0)
        .unwrap();

    lists.delete_playlist(deleted).unwrap();
    assert!(!db.root().join(cover).exists());
    assert!(!db.root().join(old_background.original_path).exists());
    assert!(!db.root().join(old_background.display_path).exists());
    assert!(db.root().join(global_background.original_path).is_file());
    assert!(db.root().join(global_background.display_path).is_file());
    assert!(tracks.audio_path(&track).unwrap().is_file());
    assert!(source.is_file());

    let reopened = Database::open(db.root()).unwrap();
    assert_eq!(
        TrackService::new(reopened.clone())
            .list_tracks()
            .unwrap()
            .as_slice(),
        std::slice::from_ref(&track)
    );
    let lists = PlaylistService::new(reopened);
    assert_eq!(
        lists
            .list_playlists()
            .unwrap()
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        [keep]
    );
    assert_eq!(lists.list_playlist_tracks(keep).unwrap(), [track]);
    assert!(lists.list_playlist_tracks(deleted).unwrap().is_empty());
    assert!(lists.delete_playlist(deleted).is_err());
}

#[test]
fn failed_playlist_deletion_preserves_appearance_and_memberships() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(root.path()).unwrap();
    let lists = PlaylistService::new(db.clone());
    let id = lists.create_playlist("Keep").unwrap();
    let conn = db.connect().unwrap();
    fs::create_dir_all(db.root().join("artwork")).unwrap();
    fs::write(db.root().join("artwork/cover.png"), b"cover").unwrap();
    conn.execute(
        "INSERT INTO playlist_appearance (playlist_id,cover_path) VALUES (?1,'artwork/cover.png')",
        [id],
    )
    .unwrap();
    conn.execute_batch("CREATE TRIGGER reject_playlist_delete BEFORE DELETE ON playlists BEGIN SELECT RAISE(ABORT,'simulated failure'); END;").unwrap();
    assert!(lists.delete_playlist(id).is_err());
    assert_eq!(
        lists.list_playlists().unwrap()[0].cover_path.as_deref(),
        Some("artwork/cover.png")
    );
    assert!(db.root().join("artwork/cover.png").is_file());
}
