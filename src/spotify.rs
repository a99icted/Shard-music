//! Public Spotify metadata only. Audio is independently matched on YouTube.
use crate::i18n::tr;
use crate::youtube::{
    DownloadedAudio, YoutubeClient, YoutubeEntry, YoutubePlaylist, YoutubeSource,
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Clone, Debug)]
pub(crate) struct SpotifyTrack {
    pub title: String,
    pub artist: String,
    pub url: url::Url,
    pub duration_ms: Option<u64>,
}

pub(crate) struct SpotifyCollection {
    pub title: String,
    pub tracks: Vec<SpotifyTrack>,
    pub is_collection: bool,
}

/// Accept only canonical public track/playlist/album links, including localized and embed links.
pub fn validate_spotify_url(input: &str) -> Result<url::Url> {
    let url = url::Url::parse(input.trim()).context(tr("Вставьте HTTPS-ссылку Spotify"))?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("open.spotify.com")
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none(),
        tr("Нужна ссылка open.spotify.com на трек, альбом или открытый плейлист")
    );
    let mut parts: Vec<_> = url
        .path_segments()
        .context(tr("Некорректная ссылка Spotify"))?
        .collect();
    if parts
        .first()
        .is_some_and(|p| p.starts_with("intl-") || *p == "embed")
    {
        parts.remove(0);
    }
    ensure!(
        parts.len() == 2
            && matches!(parts[0], "track" | "playlist" | "album")
            && parts[1].len() == 22
            && parts[1].bytes().all(|b| b.is_ascii_alphanumeric()),
        tr("Нужна ссылка open.spotify.com на трек, альбом или открытый плейлист")
    );
    Ok(url::Url::parse(&format!(
        "https://open.spotify.com/{}/{}",
        parts[0], parts[1]
    ))?)
}

pub(crate) fn fetch(url: &url::Url, cancel: &AtomicBool) -> Result<SpotifyCollection> {
    ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(25)))
        .build()
        .new_agent();
    let endpoint = format!("https://open.spotify.com/embed{}", url.path());
    let html = agent
        .get(&endpoint)
        .header("User-Agent", "Mozilla/5.0")
        .call()
        .context(tr("Не удалось получить открытые метаданные Spotify"))?
        .body_mut()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_string()
        .context(tr("Не удалось прочитать ответ Spotify"))?;
    ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
    parse_embed(&html, url)
}

fn text<'a>(value: &'a Value, field: &str) -> &'a str {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
}

pub(crate) fn parse_embed(html: &str, url: &url::Url) -> Result<SpotifyCollection> {
    let marker = html
        .find("id=\"__NEXT_DATA__\"")
        .context(tr("Spotify не предоставил открытый список треков"))?;
    let script = &html[marker..];
    let start = script.find('>').context(tr("Некорректный ответ Spotify"))? + 1;
    let end = script[start..]
        .find("</script>")
        .context(tr("Некорректный ответ Spotify"))?
        + start;
    let data: Value =
        serde_json::from_str(&script[start..end]).context(tr("Некорректные метаданные Spotify"))?;
    let entity = data
        .pointer("/props/pageProps/state/data/entity")
        .context(tr("Spotify не предоставил открытый список треков"))?;
    let is_collection = !url.path().starts_with("/track/");
    ensure!(
        text(entity, "uri")
            == format!(
                "spotify:{}",
                url.path().trim_start_matches('/').replace('/', ":")
            ),
        tr("Spotify вернул другую коллекцию")
    );
    let rows: Vec<&Value> = if is_collection {
        entity
            .get("trackList")
            .and_then(Value::as_array)
            .context(tr("Коллекция Spotify закрыта или недоступна"))?
            .iter()
            .collect()
    } else {
        vec![entity]
    };
    let mut tracks = Vec::new();
    for row in rows {
        let uri = text(row, "uri");
        let Some(id) = uri.strip_prefix("spotify:track:") else {
            continue;
        };
        let track_url = validate_spotify_url(&format!("https://open.spotify.com/track/{id}"))?;
        let title = text(row, "title");
        ensure!(!title.is_empty(), tr("Spotify вернул трек без названия"));
        let artist = if let Some(artists) = row.get("artists").and_then(Value::as_array) {
            artists
                .iter()
                .map(|a| text(a, "name"))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            text(row, "subtitle").replace('\u{a0}', " ")
        };
        ensure!(
            !artist.is_empty(),
            tr("Spotify вернул трек без исполнителя")
        );
        tracks.push(SpotifyTrack {
            title: title.into(),
            artist,
            url: track_url,
            duration_ms: row.get("duration").and_then(Value::as_u64),
        });
    }
    ensure!(!tracks.is_empty(), tr("В Spotify нет доступных треков"));
    Ok(SpotifyCollection {
        title: text(entity, "title").into(),
        tracks,
        is_collection,
    })
}

pub(crate) struct SpotifySource {
    pub youtube: YoutubeClient,
    pub collection: SpotifyCollection,
}
impl SpotifySource {
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
impl YoutubeSource for SpotifySource {
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
            .context(tr("Трек Spotify не найден"))?;
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
    #[ignore = "requires public Spotify network access"]
    fn fetches_public_track_playlist_and_album() {
        let cancel = AtomicBool::new(false);
        let track = fetch(
            &validate_spotify_url("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC").unwrap(),
            &cancel,
        )
        .unwrap();
        assert!(!track.is_collection);
        assert_eq!(track.tracks.len(), 1);
        assert_eq!(track.tracks[0].title, "Never Gonna Give You Up");
        assert_eq!(track.tracks[0].artist, "Rick Astley");
        let playlist = fetch(
            &validate_spotify_url("https://open.spotify.com/playlist/37i9dQZF1DX4dyzvuaRJ0n")
                .unwrap(),
            &cancel,
        )
        .unwrap();
        assert!(playlist.is_collection);
        assert!(playlist.tracks.len() > 50);
        assert!(!playlist.title.is_empty());
        let album = fetch(
            &validate_spotify_url(
                "https://open.spotify.com/album/5dABmDwage8L9rZ4SFpMVM?si=40d919dcb6584640",
            )
            .unwrap(),
            &cancel,
        )
        .unwrap();
        assert!(album.is_collection);
        assert_eq!(album.title, "Dead Sea Live (Acoustic Live)");
        assert_eq!(album.tracks.len(), 8);
        assert_eq!(album.tracks[0].artist, "MORGENSHTERN");
        cancel.store(true, Ordering::Relaxed);
        assert!(
            fetch(
                &validate_spotify_url("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC")
                    .unwrap(),
                &cancel
            )
            .is_err()
        );
    }
    #[test]
    fn parses_album_in_source_order() {
        let entity: Value =
            serde_json::from_str(include_str!("../tests/fixtures/spotify-album.json")).unwrap();
        let html = format!(
            "<script id=\"__NEXT_DATA__\">{}</script>",
            serde_json::json!({"props":{"pageProps":{"state":{"data":{"entity":entity}}}}})
        );
        let album = parse_embed(
            &html,
            &validate_spotify_url("https://open.spotify.com/album/5dABmDwage8L9rZ4SFpMVM?si=test")
                .unwrap(),
        )
        .unwrap();
        assert!(album.is_collection);
        assert_eq!(album.tracks.len(), 8);
        assert_eq!(album.tracks[0].title, "Красный Флаг - Acoustic Live");
        assert_eq!(album.tracks[7].title, "Последняя Любовь - Acoustic Live");
        assert_eq!(album.tracks[0].duration_ms, Some(172377));
        assert!(album.tracks.iter().all(|t| t.artist == "MORGENSHTERN"));
    }
    #[test]
    fn validates_and_normalizes_links() {
        let id = "4uLU6hMCjMI75M1A2tKUQC";
        for path in [
            format!("track/{id}?si=abc"),
            format!("intl-ru/track/{id}"),
            format!("embed/playlist/{id}"),
            format!("album/{id}?si=abc"),
            format!("intl-ru/album/{id}"),
        ] {
            assert!(validate_spotify_url(&format!("https://open.spotify.com/{path}")).is_ok());
        }
        for url in [
            "http://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC",
            "https://open.spotify.com.evil/track/4uLU6hMCjMI75M1A2tKUQC",
            "https://open.spotify.com/artist/4uLU6hMCjMI75M1A2tKUQC",
            "https://user@open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC",
        ] {
            assert!(validate_spotify_url(url).is_err());
        }
    }
    #[test]
    fn parses_track_and_playlist_without_requiring_preview_audio() {
        let url = validate_spotify_url("https://open.spotify.com/playlist/4uLU6hMCjMI75M1A2tKUQC")
            .unwrap();
        let entity = serde_json::json!({"uri":"spotify:playlist:4uLU6hMCjMI75M1A2tKUQC", "title":"Mix", "trackList":[
            {"uri":"spotify:track:4uLU6hMCjMI75M1A2tKUQC", "title":"Song & more", "subtitle":"Artist,\u{a0}Other", "duration":213000},
            {"uri":"spotify:local:missing"},
            {"uri":"spotify:track:4uLU6hMCjMI75M1A2tKUQC", "title":"Song & more", "subtitle":"Artist, Other"}]});
        let html = format!(
            "<script id=\"__NEXT_DATA__\" type=\"application/json\">{}</script>",
            serde_json::json!({"props":{"pageProps":{"state":{"data":{"entity":entity}}}}})
        );
        let parsed = parse_embed(&html, &url).unwrap();
        assert_eq!(parsed.title, "Mix");
        assert_eq!(parsed.tracks.len(), 2);
        assert_eq!(parsed.tracks[0].artist, "Artist, Other");
        assert_eq!(parsed.tracks[0].duration_ms, Some(213000));
        assert!(parse_embed("private", &url).is_err());
        assert!(
            parse_embed(
                &html,
                &validate_spotify_url("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC")
                    .unwrap()
            )
            .is_err()
        );
    }
}
