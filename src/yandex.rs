//! Public Yandex Music metadata. Audio is matched and downloaded from YouTube.
use crate::{
    i18n::tr,
    music_metadata::{Collection, CollectionTrack},
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub(crate) fn is_host(host: &str) -> bool {
    matches!(
        host,
        "music.yandex.ru"
            | "music.yandex.com"
            | "music.yandex.kz"
            | "music.yandex.by"
            | "music.yandex.uz"
    )
}

pub fn validate_yandex_url(input: &str) -> Result<url::Url> {
    let mut url =
        url::Url::parse(input.trim()).context(tr("Вставьте HTTPS-ссылку Яндекс Музыки"))?;
    ensure!(
        url.scheme() == "https"
            && url.host_str().is_some_and(is_host)
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none(),
        tr("Нужна ссылка music.yandex.ru на трек, альбом или открытый плейлист")
    );
    let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
    let number = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let valid = match parts.as_slice() {
        ["album", id] | ["track", id] => number(id),
        ["album", album, "track", track] => number(album) && number(track),
        ["users", user, "playlists", kind] => !user.is_empty() && user.len() <= 200 && number(kind),
        ["playlists", id] => {
            let uuid = id
                .strip_prefix("ge.")
                .or_else(|| id.strip_prefix("lk."))
                .unwrap_or(id);
            uuid::Uuid::parse_str(uuid).is_ok()
        }
        _ => false,
    };
    ensure!(
        valid,
        tr("Нужна ссылка music.yandex.ru на трек, альбом или открытый плейлист")
    );
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

pub(crate) fn fetch(url: &url::Url, cancel: &AtomicBool) -> Result<Collection> {
    let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
    let (endpoint, collection) = match parts.as_slice() {
        ["track", id] | ["album", _, "track", id] => (format!("tracks/{id}"), false),
        ["album", id] => (format!("albums/{id}/with-tracks"), true),
        ["users", user, "playlists", kind] => (format!("users/{user}/playlists/{kind}"), true),
        ["playlists", id] => (format!("playlist/{id}"), true),
        _ => anyhow::bail!(tr("Некорректная ссылка Яндекс Музыки")),
    };
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .new_agent();
    let get = |endpoint: &str| -> Result<Value> {
        ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
        let body = agent
            .get(&format!("https://api.music.yandex.net/{endpoint}"))
            .header("User-Agent", "Shard/0.1")
            .call()
            .context(tr("Не удалось получить открытые метаданные Яндекс Музыки"))?
            .body_mut()
            .with_config()
            .limit(32 * 1024 * 1024)
            .read_to_string()?;
        ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
        let data: Value = serde_json::from_str(&body)?;
        ensure!(
            data.get("error").is_none(),
            tr("Коллекция Яндекс Музыки закрыта или недоступна")
        );
        data.get("result")
            .cloned()
            .context(tr("Некорректный ответ Яндекс Музыки"))
    };
    let mut result = get(&endpoint)?;
    // Playlist entries can contain only IDs. Fetch their full metadata in bounded batches.
    if let Some(rows) = result.get_mut("tracks").and_then(Value::as_array_mut) {
        let missing: Vec<_> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.get("track").is_none())
            .map(|(i, row)| Ok((i, id(row.get("id").context("Missing Yandex track ID")?)?)))
            .collect::<Result<_>>()?;
        for batch in missing.chunks(50) {
            let ids = batch
                .iter()
                .map(|(_, id)| id.as_str())
                .collect::<Vec<_>>()
                .join(",");
            let metadata = get(&format!("tracks/{ids}"))?;
            for (index, requested) in batch {
                let track = metadata
                    .as_array()
                    .and_then(|tracks| {
                        tracks.iter().find(|track| {
                            track.get("id").and_then(|v| id(v).ok()).as_ref() == Some(requested)
                        })
                    })
                    .context(tr("Яндекс Музыка не предоставила все треки плейлиста"))?;
                rows[*index]["track"] = track.clone();
            }
        }
    }
    parse(&result, collection)
}

fn id(value: &Value) -> Result<String> {
    let id = value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|n| n.to_string()))
        .context("Invalid Yandex track ID")?;
    ensure!(
        !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()),
        "Invalid Yandex track ID"
    );
    Ok(id)
}

fn yandex_artwork(value: &Value) -> Option<String> {
    value
        .get("coverUri")
        .or_else(|| value.pointer("/cover/uri"))
        .or_else(|| value.pointer("/albums/0/coverUri"))
        .and_then(Value::as_str)
        .map(|uri| {
            let uri = uri.replace("%%", "600x600");
            if uri.starts_with("https://") {
                uri
            } else {
                format!("https://{uri}")
            }
        })
}

fn parse(result: &Value, is_collection: bool) -> Result<Collection> {
    let title = result
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Яндекс Музыка")
        .to_owned();
    let rows: Vec<&Value> = if !is_collection {
        result
            .as_array()
            .context(tr("Некорректный ответ Яндекс Музыки"))?
            .iter()
            .collect()
    } else if let Some(volumes) = result.get("volumes").and_then(Value::as_array) {
        volumes
            .iter()
            .filter_map(Value::as_array)
            .flatten()
            .collect()
    } else {
        result
            .get("tracks")
            .and_then(Value::as_array)
            .context(tr("Коллекция Яндекс Музыки закрыта или недоступна"))?
            .iter()
            .collect()
    };
    let mut tracks = Vec::new();
    for row in rows {
        let track = row.get("track").unwrap_or(row);
        let track_id = id(track.get("id").context("Missing Yandex track ID")?)?;
        let name = track
            .get("title")
            .and_then(Value::as_str)
            .context(tr("Трек без названия"))?;
        let artist = track
            .get("artists")
            .and_then(Value::as_array)
            .context(tr("Трек без исполнителя"))?
            .iter()
            .filter_map(|a| a.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(", ");
        ensure!(
            !name.trim().is_empty() && !artist.trim().is_empty(),
            tr("Яндекс Музыка вернула неполные метаданные трека")
        );
        let name = match track
            .get("version")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
        {
            Some(version) => format!("{name} ({version})"),
            None => name.into(),
        };
        tracks.push(CollectionTrack {
            artwork_url: yandex_artwork(track),
            title: name,
            artist,
            url: url::Url::parse(&format!("https://music.yandex.ru/track/{track_id}"))?,
            duration_ms: track.get("durationMs").and_then(Value::as_u64),
        });
    }
    if let Some(expected) = result.get("trackCount").and_then(Value::as_u64) {
        ensure!(
            tracks.len() as u64 == expected,
            tr("Яндекс Музыка не предоставила все треки плейлиста")
        );
    }
    ensure!(
        !tracks.is_empty(),
        tr("В Яндекс Музыке нет доступных треков")
    );
    Ok(Collection {
        artwork_url: yandex_artwork(result),
        title,
        tracks,
        is_collection,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_music_links_and_rejects_untrusted_hosts_and_paths() {
        for path in [
            "album/123",
            "track/456",
            "album/123/track/456",
            "users/user.name/playlists/1000",
            "playlists/12345678-1234-4234-8234-123456789abc",
            "playlists/ge.12345678-1234-4234-8234-123456789abc",
        ] {
            assert!(
                validate_yandex_url(&format!("https://music.yandex.ru/{path}?utm_source=share"))
                    .is_ok()
            );
        }
        for url in [
            "http://music.yandex.ru/track/1",
            "https://music.yandex.ru.evil/track/1",
            "https://user@music.yandex.ru/track/1",
            "https://music.yandex.ru/artist/1",
            "https://music.yandex.ru/track/../1",
        ] {
            assert!(validate_yandex_url(url).is_err());
        }
    }
    #[test]
    fn parses_ordered_album_volumes_playlist_wrappers_and_versions() {
        let track = serde_json::json!({"id":123,"title":"Song","version":"Live","artists":[{"name":"Artist"},{"name":"Guest"}],"durationMs":123000});
        let album = parse(
            &serde_json::json!({"title":"Album","volumes":[[track.clone()],[track.clone()]]}),
            true,
        )
        .unwrap();
        assert_eq!(album.tracks.len(), 2);
        assert_eq!(album.tracks[0].title, "Song (Live)");
        assert_eq!(album.tracks[0].artist, "Artist, Guest");
        let playlist = parse(
            &serde_json::json!({"title":"Mix","trackCount":1,"tracks":[{"track":track}]}),
            true,
        )
        .unwrap();
        assert_eq!(
            playlist.tracks[0].url.as_str(),
            "https://music.yandex.ru/track/123"
        );
        assert!(parse(&serde_json::json!({"tracks":[],"trackCount":1}), true).is_err());
    }
    #[test]
    #[ignore = "requires public Yandex Music network access"]
    fn fetches_public_track_album_and_both_playlist_link_formats() {
        for input in [
            "https://music.yandex.ru/album/1",
            "https://music.yandex.ru/track/31317367",
            "https://music.yandex.ru/users/yamusic-top/playlists/1000",
            "https://music.yandex.ru/playlists/ge.d7aed00b-5de3-4973-873d-a950bd03e0fb",
        ] {
            let collection = fetch(
                &validate_yandex_url(input).unwrap(),
                &AtomicBool::new(false),
            )
            .unwrap();
            assert!(!collection.tracks.is_empty(), "{input}");
            if input.contains("playlists/") {
                assert_eq!(collection.tracks.len(), 100);
            }
        }
    }
}
