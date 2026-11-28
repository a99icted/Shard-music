use super::*;
use crate::youtube::{DownloadedAudio, YoutubeEntry};
use std::{cell::RefCell, io::Write};

struct FakeYoutube {
    calls: RefCell<Vec<String>>,
    cancel_on: Option<&'static str>,
}

impl YoutubeSource for FakeYoutube {
    fn playlist(&self, _: &url::Url, _: &AtomicBool) -> Result<YoutubePlaylist> {
        Ok(YoutubePlaylist {
            title: "  Моя подборка  ".into(),
            entries: [Some("first"), Some("private"), None, Some("last")]
                .into_iter()
                .enumerate()
                .map(|(index, id)| YoutubeEntry {
                    title: format!("Entry {index}"),
                    url: id.map(|id| {
                        url::Url::parse(&format!("https://youtube.com/watch?v={id}")).unwrap()
                    }),
                    ..Default::default()
                })
                .collect(),
        })
    }
    fn track(
        &self,
        url: &url::Url,
        staging: &Path,
        cancel: &AtomicBool,
    ) -> Result<DownloadedAudio> {
        let id = url
            .query_pairs()
            .find(|(key, _)| key == "v")
            .map(|(_, value)| value.into_owned())
            .unwrap_or_else(|| url.path().trim_start_matches('/').to_owned());
        self.calls.borrow_mut().push(id.clone());
        if self.cancel_on == Some(id.as_str()) {
            cancel.store(true, Ordering::Relaxed);
            anyhow::bail!("cancelled");
        }
        if id == "private" {
            anyhow::bail!("yt-dlp: Private video");
        }
        let path = staging.join("audio.wav");
        let mut file = fs::File::create(&path)?;
        file.write_all(b"RIFF")?;
        file.write_all(&16036u32.to_le_bytes())?;
        file.write_all(b"WAVEfmt ")?;
        file.write_all(&16u32.to_le_bytes())?;
        file.write_all(&1u16.to_le_bytes())?;
        file.write_all(&1u16.to_le_bytes())?;
        file.write_all(&8000u32.to_le_bytes())?;
        file.write_all(&16000u32.to_le_bytes())?;
        file.write_all(&2u16.to_le_bytes())?;
        file.write_all(&16u16.to_le_bytes())?;
        file.write_all(b"data")?;
        file.write_all(&16000u32.to_le_bytes())?;
        file.write_all(&vec![0; 16000])?;
        Ok(DownloadedAudio {
            path,
            info: DownloadInfo {
                title: Some(id),
                ..Default::default()
            },
        })
    }
}

fn fake(cancel_on: Option<&'static str>) -> FakeYoutube {
    FakeYoutube {
        calls: RefCell::new(vec![]),
        cancel_on,
    }
}

#[test]
fn playlist_import_continues_after_failures_and_persists_original_order() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(root.path()).unwrap();
    let service = TrackService::new(db.clone());
    let source = fake(None);
    let mut progress = vec![];
    let report = service
        .download_youtube_with_source(
            &validate_youtube_url("https://youtube.com/watch?v=last&list=PLtest").unwrap(),
            &source,
            &AtomicBool::new(false),
            |event| progress.push(event),
        )
        .unwrap();
    assert_eq!(
        source.calls.borrow().as_slice(),
        ["first", "private", "last"]
    );
    assert_eq!(report.total, 4);
    assert_eq!(report.tracks.len(), 2);
    assert_eq!(report.failures.len(), 2);
    assert!(report.error_details().unwrap().contains("Private video"));
    assert!(!report.cancelled);
    let final_progress = progress.last().unwrap();
    assert_eq!((final_progress.completed, final_progress.total), (4, 4));
    assert_eq!(
        fs::read_dir(root.path().join("staging")).unwrap().count(),
        0
    );
    drop(service);
    let reopened = Database::open(root.path()).unwrap();
    let lists = PlaylistService::new(reopened.clone());
    assert_eq!(lists.list_playlists().unwrap()[0].name, "Моя подборка");
    let playlist_tracks = lists
        .list_playlist_tracks(report.playlist_id.unwrap())
        .unwrap();
    assert_eq!(
        playlist_tracks
            .iter()
            .map(|t| t.title.as_str())
            .collect::<Vec<_>>(),
        ["first", "last"]
    );
    assert_eq!(TrackService::new(reopened).list_tracks().unwrap().len(), 2);
}

#[test]
fn cancellation_keeps_completed_tracks_and_does_not_start_remaining_downloads() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(root.path()).unwrap();
    let service = TrackService::new(db.clone());
    let source = fake(Some("private"));
    let report = service
        .download_youtube_with_source(
            &validate_youtube_url("https://youtube.com/playlist?list=PLtest").unwrap(),
            &source,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    assert!(report.cancelled);
    assert_eq!(report.tracks.len(), 1);
    assert!(report.failures.is_empty());
    assert_eq!(source.calls.borrow().as_slice(), ["first", "private"]);
    assert_eq!(
        PlaylistService::new(db)
            .list_playlist_tracks(report.playlist_id.unwrap())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        fs::read_dir(root.path().join("staging")).unwrap().count(),
        0
    );
}

#[test]
fn a_single_video_does_not_create_a_local_playlist() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(root.path()).unwrap();
    let service = TrackService::new(db.clone());
    let report = service
        .download_youtube_with_source(
            &validate_youtube_url("https://youtube.com/watch?v=first").unwrap(),
            &fake(None),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    assert_eq!(report.tracks.len(), 1);
    assert_eq!(report.total, 1);
    assert!(report.playlist_id.is_none());
    assert!(
        PlaylistService::new(db)
            .list_playlists()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn cancellation_before_start_leaves_no_empty_playlist() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(root.path()).unwrap();
    let service = TrackService::new(db.clone());
    let source = fake(None);
    let report = service
        .download_youtube_with_source(
            &validate_youtube_url("https://youtube.com/playlist?list=PLtest").unwrap(),
            &source,
            &AtomicBool::new(true),
            |_| {},
        )
        .unwrap();
    assert!(report.cancelled);
    assert!(source.calls.borrow().is_empty());
    assert!(
        PlaylistService::new(db)
            .list_playlists()
            .unwrap()
            .is_empty()
    );
}

fn download_entry(id: &str, title: &str, artist: &str, duration_ms: Option<i64>) -> YoutubeEntry {
    YoutubeEntry {
        title: title.into(),
        artist: artist.into(),
        duration_ms,
        url: Some(validate_youtube_url(&format!("https://youtube.com/watch?v={id}")).unwrap()),
    }
}

fn seed_download(service: &TrackService, source: &FakeYoutube, id: &str) -> Track {
    let staging = tempfile::tempdir_in(service.db.root().join("staging")).unwrap();
    let url = validate_youtube_url(&format!("https://youtube.com/watch?v={id}")).unwrap();
    let audio = source
        .track(&url, staging.path(), &AtomicBool::new(false))
        .unwrap();
    service
        .import_file(&audio.path, Some(url.to_string()), Some(audio.info))
        .unwrap()
}

#[test]
fn search_album_creates_named_collection_with_order_and_reused_tracks() {
    use crate::youtube::search::{CollectionDownload, ResultKind, SearchResult};
    for reuse in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(root.path()).unwrap();
        let service = TrackService::new(db.clone());
        let source = fake(None);
        let mut existing = Vec::new();
        if reuse {
            for id in ["last", "first"] {
                let track = seed_download(&service, &source, id);
                let cover = crate::artwork::track_path(root.path(), &track);
                fs::create_dir_all(cover.parent().unwrap()).unwrap();
                image::RgbImage::from_pixel(8, 8, image::Rgb([20, 80, 160]))
                    .save(cover)
                    .unwrap();
                existing.push(track.id);
            }
        }
        source.calls.borrow_mut().clear();
        let result = |id: &str, title: &str, kind| SearchResult {
            id: id.into(),
            title: title.into(),
            artist: "Artist".into(),
            url: format!("https://youtube.com/watch?v={id}"),
            thumbnail: None,
            duration_ms: None,
            play_count: None,
            kind,
        };
        let mut album = result("MPREalbum", "  Найденный альбом  ", ResultKind::Album);
        album.url = "https://music.youtube.com/browse/MPREalbum".into();
        let download = CollectionDownload {
            collection: album,
            tracks: ["last", "private", "first"]
                .into_iter()
                .map(|id| result(id, id, ResultKind::Track))
                .collect(),
        };
        let report = service
            .download_search_collection_with_source(
                &download,
                &source,
                &AtomicBool::new(false),
                |_| {},
                DownloadOptions {
                    warn_duplicates: false,
                    ignore_duplicates: reuse,
                },
                &mut |_| panic!("Warnings disabled"),
            )
            .unwrap();
        assert_eq!(report.total, 3);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.reused, if reuse { 2 } else { 0 });
        assert_eq!(report.tracks.len(), if reuse { 0 } else { 2 });
        let id = report
            .playlist_id
            .expect("Download all must create a collection");
        let lists = PlaylistService::new(db.clone());
        let playlist = lists
            .list_playlists()
            .unwrap()
            .into_iter()
            .find(|p| p.id == id)
            .unwrap();
        assert_eq!(playlist.name, "Найденный альбом");
        let tracks = lists.list_playlist_tracks(id).unwrap();
        assert_eq!(
            tracks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
            ["last", "first"]
        );
        assert_eq!(service.list_tracks().unwrap().len(), 2);
        if reuse {
            assert_eq!(tracks.iter().map(|t| t.id).collect::<Vec<_>>(), existing);
            assert_eq!(source.calls.borrow().as_slice(), ["private"]);
            assert!(
                root.path()
                    .join(playlist.cover_path.expect("Reuse the track artwork"))
                    .is_file()
            );
        } else {
            assert_eq!(
                source.calls.borrow().as_slice(),
                ["last", "private", "first"]
            );
        }
    }
}

#[test]
fn download_duplicate_review_reuses_library_and_honors_individual_choices() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(root.path()).unwrap();
    let service = TrackService::new(db.clone());
    let source = fake(None);
    let first = seed_download(&service, &source, "first");
    seed_download(&service, &source, "last");
    source.calls.borrow_mut().clear();
    let mut reviews = 0;
    let report = service
        .download_entries(
            YoutubePlaylist {
                title: "Duplicates".into(),
                entries: vec![
                    download_entry("first", "First", "Artist", None),
                    download_entry("last", "Last", "Artist", None),
                ],
            },
            true,
            &source,
            &AtomicBool::new(false),
            |_| {},
            DownloadOptions::default(),
            &mut |duplicates| {
                reviews += 1;
                assert!(
                    source.calls.borrow().is_empty(),
                    "review happens before downloading audio"
                );
                assert_eq!(duplicates.len(), 2);
                assert!(
                    duplicates
                        .iter()
                        .all(|d| d.kind == DuplicateKind::SameSource)
                );
                Ok(vec![
                    DuplicateDecision {
                        index: 0,
                        copy: false,
                    },
                    DuplicateDecision {
                        index: 1,
                        copy: true,
                    },
                ])
            },
        )
        .unwrap();
    assert_eq!(reviews, 1);
    assert_eq!(source.calls.borrow().as_slice(), ["last"]);
    assert_eq!((report.tracks.len(), report.reused), (1, 1));
    assert!(report.failures.is_empty());
    assert_eq!(service.list_tracks().unwrap().len(), 3);
    let tracks = PlaylistService::new(db)
        .list_playlist_tracks(report.playlist_id.unwrap())
        .unwrap();
    assert_eq!(
        tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
        [first.id, report.tracks[0].id]
    );
}

#[test]
fn download_repeated_sources_follow_both_automatic_policies_without_prompting() {
    for ignore_duplicates in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(root.path()).unwrap();
        let service = TrackService::new(db.clone());
        let source = fake(None);
        let mut repeated = download_entry("first", "Repeated", "", None);
        repeated.url = Some(validate_youtube_url("https://youtu.be/first?t=10").unwrap());
        let report = service
            .download_entries(
                YoutubePlaylist {
                    title: "Repeated entries".into(),
                    entries: vec![
                        download_entry("first", "First", "", None),
                        repeated,
                        download_entry("first", "Again", "", None),
                    ],
                },
                true,
                &source,
                &AtomicBool::new(false),
                |_| {},
                DownloadOptions {
                    warn_duplicates: false,
                    ignore_duplicates,
                },
                &mut |_| panic!("warnings are disabled"),
            )
            .unwrap();
        let expected = if ignore_duplicates { 1 } else { 3 };
        assert_eq!(source.calls.borrow().len(), expected);
        assert_eq!(report.tracks.len(), expected);
        assert_eq!(report.reused, 3 - expected);
        assert!(report.failures.is_empty());
        assert_eq!(service.list_tracks().unwrap().len(), expected);
        assert_eq!(
            PlaylistService::new(db)
                .list_playlist_tracks(report.playlist_id.unwrap())
                .unwrap()
                .len(),
            expected
        );
    }
}

#[test]
fn download_metadata_matches_reuse_local_imports_before_downloading() {
    let root = tempfile::tempdir().unwrap();
    let service = TrackService::new(Database::open(root.path()).unwrap());
    let source = fake(None);
    let track = seed_download(&service, &source, "original");
    service
        .update_metadata(track.id, "  Song ", "ARTIST")
        .unwrap();
    source.calls.borrow_mut().clear();
    let report = service
        .download_entries(
            YoutubePlaylist {
                title: String::new(),
                entries: vec![download_entry("other", "song", "artist", Some(3000))],
            },
            false,
            &source,
            &AtomicBool::new(false),
            |_| {},
            DownloadOptions::default(),
            &mut |duplicates| {
                assert_eq!(duplicates.len(), 1);
                assert_eq!(duplicates[0].kind, DuplicateKind::SimilarMetadata);
                Ok(vec![DuplicateDecision {
                    index: 0,
                    copy: false,
                }])
            },
        )
        .unwrap();
    assert!(source.calls.borrow().is_empty());
    assert_eq!((report.tracks.len(), report.reused), (0, 1));
    assert!(report.playlist_id.is_none());
}

#[test]
fn cancelling_download_duplicate_review_does_not_create_a_playlist_or_audio() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(root.path()).unwrap();
    let service = TrackService::new(db.clone());
    let source = fake(None);
    seed_download(&service, &source, "first");
    source.calls.borrow_mut().clear();
    let cancel = AtomicBool::new(false);
    let report = service
        .download_entries(
            YoutubePlaylist {
                title: "Cancelled".into(),
                entries: vec![download_entry("first", "First", "", None)],
            },
            true,
            &source,
            &cancel,
            |_| {},
            DownloadOptions::default(),
            &mut |_| {
                cancel.store(true, Ordering::Relaxed);
                anyhow::bail!("cancelled review")
            },
        )
        .unwrap();
    assert!(report.cancelled);
    assert!(report.failures.is_empty());
    assert!(source.calls.borrow().is_empty());
    assert_eq!(service.list_tracks().unwrap().len(), 1);
    assert!(
        PlaylistService::new(db)
            .list_playlists()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn download_identical_file_is_reviewed_before_import_and_staging_is_cleaned() {
    let root = tempfile::tempdir().unwrap();
    let service = TrackService::new(Database::open(root.path()).unwrap());
    let source = fake(None);
    seed_download(&service, &source, "original");
    source.calls.borrow_mut().clear();
    let report = service
        .download_entries(
            YoutubePlaylist {
                title: String::new(),
                entries: vec![download_entry("different", "Different", "", None)],
            },
            false,
            &source,
            &AtomicBool::new(false),
            |_| {},
            DownloadOptions::default(),
            &mut |duplicates| {
                assert_eq!(source.calls.borrow().as_slice(), ["different"]);
                assert_eq!(duplicates.len(), 1);
                assert_eq!(duplicates[0].kind, DuplicateKind::IdenticalFile);
                Ok(vec![DuplicateDecision {
                    index: 0,
                    copy: false,
                }])
            },
        )
        .unwrap();
    assert_eq!((report.tracks.len(), report.reused), (0, 1));
    assert!(report.failures.is_empty());
    assert_eq!(service.list_tracks().unwrap().len(), 1);
    assert_eq!(
        fs::read_dir(root.path().join("staging")).unwrap().count(),
        0
    );
    assert!(report.message().contains('1'));
}

#[test]
fn download_failed_earlier_match_falls_back_to_downloading_the_later_entry() {
    let root = tempfile::tempdir().unwrap();
    let service = TrackService::new(Database::open(root.path()).unwrap());
    let source = fake(None);
    let report = service
        .download_entries(
            YoutubePlaylist {
                title: "Fallback".into(),
                entries: vec![
                    download_entry("private", "Song", "Artist", Some(1000)),
                    download_entry("first", "Song", "Artist", Some(1000)),
                ],
            },
            true,
            &source,
            &AtomicBool::new(false),
            |_| {},
            DownloadOptions {
                warn_duplicates: false,
                ignore_duplicates: true,
            },
            &mut |_| unreachable!(),
        )
        .unwrap();
    assert_eq!(source.calls.borrow().as_slice(), ["private", "first"]);
    assert_eq!(
        (report.tracks.len(), report.reused, report.failures.len()),
        (1, 0, 1)
    );
}

#[test]
fn download_does_not_reuse_missing_audio_or_accept_incomplete_decisions() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(root.path()).unwrap();
    let service = TrackService::new(db.clone());
    let source = fake(None);
    let missing = seed_download(&service, &source, "first");
    fs::remove_file(service.audio_path(&missing).unwrap()).unwrap();
    let report = service
        .download_entries(
            YoutubePlaylist {
                title: String::new(),
                entries: vec![download_entry("first", "First", "", None)],
            },
            false,
            &source,
            &AtomicBool::new(false),
            |_| {},
            DownloadOptions {
                warn_duplicates: false,
                ignore_duplicates: true,
            },
            &mut |_| unreachable!(),
        )
        .unwrap();
    assert_eq!((report.tracks.len(), report.reused), (1, 0));
    assert!(service.audio_path(&report.tracks[0]).unwrap().is_file());
    source.calls.borrow_mut().clear();
    assert!(
        service
            .download_entries(
                YoutubePlaylist {
                    title: "Invalid decision".into(),
                    entries: vec![download_entry("first", "First", "", None)],
                },
                true,
                &source,
                &AtomicBool::new(false),
                |_| {},
                DownloadOptions::default(),
                &mut |_| Ok(vec![])
            )
            .is_err()
    );
    assert!(source.calls.borrow().is_empty());
    assert!(
        PlaylistService::new(db)
            .list_playlists()
            .unwrap()
            .is_empty()
    );
}
