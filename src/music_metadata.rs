//! Shared metadata-to-YouTube matching for music services.
use crate::{
    i18n::tr,
    youtube::{
        DownloadInfo, DownloadedAudio, YoutubeClient, YoutubeEntry, YoutubePlaylist, YoutubeSource,
    },
};
use anyhow::{Context, Result, ensure};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Debug)]
pub(crate) struct CollectionTrack {
    pub artwork_url: Option<String>,
    pub title: String,
    pub artist: String,
    pub url: url::Url,
    pub duration_ms: Option<u64>,
}

pub(crate) struct Collection {
    pub artwork_url: Option<String>,
    pub title: String,
    pub tracks: Vec<CollectionTrack>,
    pub is_collection: bool,
    // Album art can represent its tracks; a Spotify playlist cover cannot.
    pub use_collection_artwork: bool,
}

impl Collection {
    fn apply_track_metadata(&self, metadata: &CollectionTrack, info: &mut DownloadInfo) {
        info.track = Some(metadata.title.clone());
        info.artist = Some(metadata.artist.clone());
        if !self.use_collection_artwork {
            info.thumbnail_fallback = info.thumbnail.clone().filter(|youtube| {
                metadata
                    .artwork_url
                    .as_ref()
                    .is_some_and(|artwork| artwork != youtube)
            });
        }
        info.thumbnail = metadata
            .artwork_url
            .clone()
            .or_else(|| {
                self.use_collection_artwork
                    .then(|| self.artwork_url.clone())
                    .flatten()
            })
            .or_else(|| info.thumbnail.take());
    }
}

pub(crate) struct MetadataSource {
    pub youtube: YoutubeClient,
    pub collection: Collection,
}
impl MetadataSource {
    pub fn entries(&self) -> YoutubePlaylist {
        YoutubePlaylist {
            title: self.collection.title.clone(),
            entries: self
                .collection
                .tracks
                .iter()
                .map(|track| YoutubeEntry {
                    title: track.title.clone(),
                    artist: track.artist.clone(),
                    duration_ms: track.duration_ms.and_then(|v| i64::try_from(v).ok()),
                    url: Some(track.url.clone()),
                })
                .collect(),
        }
    }
}
impl YoutubeSource for MetadataSource {
    fn artwork_url(&self) -> Option<&str> {
        self.collection.artwork_url.as_deref()
    }
    fn playlist(&self, _: &url::Url, _: &AtomicBool) -> Result<YoutubePlaylist> {
        Ok(self.entries())
    }
    fn track(
        &self,
        url: &url::Url,
        staging: &Path,
        cancel: &AtomicBool,
    ) -> Result<DownloadedAudio> {
        let metadata = self
            .collection
            .tracks
            .iter()
            .find(|t| &t.url == url)
            .context(tr("Трек не найден"))?;
        let candidates = self.youtube.search_candidates(
            &metadata.title,
            &metadata.artist,
            metadata.duration_ms,
            cancel,
        )?;
        let mut last_error = None;
        for video in candidates {
            ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
            // Isolate every attempt so partial audio or JSON from a failed video
            // cannot be reused by yt-dlp for a different match.
            let attempt = tempfile::tempdir_in(staging)?;
            match self.youtube.track(&video, attempt.path(), cancel) {
                Ok(mut audio) => {
                    let _ = attempt.keep();
                    self.collection
                        .apply_track_metadata(metadata, &mut audio.info);
                    return Ok(audio);
                }
                Err(error) => last_error = Some(error),
            }
        }
        ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
        Err(last_error.expect("search_candidates always returns at least one match"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spotify_playlist_uses_track_artwork_or_youtube_and_keeps_playlist_cover_separate() {
        let mut track = CollectionTrack {
            artwork_url: None,
            title: "Song".into(),
            artist: "Artist".into(),
            url: url::Url::parse("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC").unwrap(),
            duration_ms: None,
        };
        let mut collection = Collection {
            artwork_url: Some("https://example.com/playlist.jpg".into()),
            title: "Mix".into(),
            tracks: vec![],
            is_collection: true,
            use_collection_artwork: false,
        };
        for cover in [None, Some("https://example.com/song.jpg")] {
            track.artwork_url = cover.map(str::to_owned);
            let mut info = DownloadInfo {
                thumbnail: Some("https://example.com/youtube.jpg".into()),
                ..Default::default()
            };
            collection.apply_track_metadata(&track, &mut info);
            assert_eq!(
                info.thumbnail.as_deref(),
                Some(cover.unwrap_or("https://example.com/youtube.jpg"))
            );
            assert_eq!(
                info.thumbnail_fallback.as_deref(),
                cover.map(|_| "https://example.com/youtube.jpg")
            );
            assert_eq!(info.track.as_deref(), Some("Song"));
            assert_eq!(
                collection.artwork_url.as_deref(),
                Some("https://example.com/playlist.jpg")
            );
        }
        let mut no_artwork = DownloadInfo::default();
        track.artwork_url = None;
        collection.apply_track_metadata(&track, &mut no_artwork);
        assert!(no_artwork.thumbnail.is_none());

        // Album imports retain the existing collection-cover priority.
        collection.use_collection_artwork = true;
        let mut album = DownloadInfo {
            thumbnail: Some("https://example.com/youtube.jpg".into()),
            ..Default::default()
        };
        collection.apply_track_metadata(&track, &mut album);
        assert_eq!(album.thumbnail, collection.artwork_url);
        assert!(album.thumbnail_fallback.is_none());
        track.artwork_url = Some("https://example.com/song.jpg".into());
        collection.apply_track_metadata(&track, &mut album);
        assert_eq!(album.thumbnail, track.artwork_url);
    }

    #[test]
    #[ignore = "requires Spotify/YouTube network access and download tools"]
    fn downloads_spotify_playlist_and_album_samples_with_distinct_artwork_rules() {
        let cancel = AtomicBool::new(false);
        for (kind, id) in [
            ("playlist", "3g3ZXjjQgq3NGT8gI1QhqV"),
            ("album", "5dABmDwage8L9rZ4SFpMVM"),
        ] {
            let url = crate::spotify::validate_spotify_url(&format!(
                "https://open.spotify.com/{kind}/{id}"
            ))
            .unwrap();
            let collection = crate::spotify::fetch(&url, &cancel).unwrap();
            let track = collection.tracks.first().unwrap().clone();
            let staging = tempfile::tempdir().unwrap();
            let source = MetadataSource {
                youtube: YoutubeClient::new(staging.path()).unwrap(),
                collection,
            };
            let audio = source.track(&track.url, staging.path(), &cancel).unwrap();
            assert!(std::fs::metadata(&audio.path).unwrap().len() > 10_000);
            let cover = audio.info.thumbnail.as_deref().unwrap();
            assert_eq!(Some(cover), track.artwork_url.as_deref());
            if kind == "playlist" {
                assert_ne!(Some(cover), source.collection.artwork_url.as_deref());
            } else {
                assert_eq!(Some(cover), source.collection.artwork_url.as_deref());
            }
            let bytes = crate::artwork::download(cover).unwrap();
            assert!(image::load_from_memory(&bytes).is_ok());
            println!(
                "Downloaded {kind} sample: {} — {}; audio {} bytes; artwork decoded",
                track.artist,
                track.title,
                std::fs::metadata(audio.path).unwrap().len()
            );
        }
    }
}
