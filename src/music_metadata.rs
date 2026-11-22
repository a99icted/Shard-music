//! Shared metadata-to-YouTube matching for music services.
use crate::{
    i18n::tr,
    youtube::{DownloadedAudio, YoutubeClient, YoutubeEntry, YoutubePlaylist, YoutubeSource},
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
                    title: format!("{} — {}", track.artist, track.title),
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
                    audio.info.track = Some(metadata.title.clone());
                    audio.info.artist = Some(metadata.artist.clone());
                    audio.info.thumbnail = metadata
                        .artwork_url
                        .clone()
                        .or_else(|| self.collection.artwork_url.clone())
                        .or(audio.info.thumbnail);
                    return Ok(audio);
                }
                Err(error) => last_error = Some(error),
            }
        }
        ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
        Err(last_error.expect("search_candidates always returns at least one match"))
    }
}
