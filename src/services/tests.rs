use super::*;
use crate::youtube::DownloadedAudio;
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
            .unwrap()
            .1
            .into_owned();
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
