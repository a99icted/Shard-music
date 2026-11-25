//! Public Spotify metadata only. Audio is independently matched on YouTube.
use crate::i18n::tr;
use crate::music_metadata::{Collection, CollectionTrack};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

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

pub(crate) fn fetch(url: &url::Url, cancel: &AtomicBool) -> Result<Collection> {
    ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(25)))
        .build()
        .new_agent();
    let endpoint = format!("https://open.spotify.com/embed{}", url.path());
    let html = fetch_embed_html(&agent, &endpoint, cancel)?;
    ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
    parse_embed(&html, url)
}

fn fetch_embed_html(agent: &ureq::Agent, endpoint: &str, cancel: &AtomicBool) -> Result<String> {
    // A gateway can fail transiently even for a public playlist. Retry only
    // temporary HTTP failures, keeping private/missing collections terminal.
    for attempt in 0..3 {
        ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
        let response = agent
            .get(endpoint)
            .header("User-Agent", "Mozilla/5.0")
            .call();
        let mut response = match response {
            Err(ureq::Error::StatusCode(status @ (502 | 503 | 504))) => {
                if attempt == 2 {
                    anyhow::bail!(
                        "{} (HTTP {status}; 3 attempts)",
                        tr("Временная ошибка Spotify при получении метаданных")
                    );
                }
                // Short intervals let Stop interrupt the retry delay.
                for _ in 0..(5 * (attempt + 1)) {
                    ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
                    std::thread::sleep(Duration::from_millis(100));
                }
                continue;
            }
            response => response.context(tr("Не удалось получить открытые метаданные Spotify"))?,
        };
        return response
            .body_mut()
            .with_config()
            .limit(16 * 1024 * 1024)
            .read_to_string()
            .context(tr("Не удалось прочитать ответ Spotify"));
    }
    unreachable!("the last attempt returns a response or an error")
}

fn text<'a>(value: &'a Value, field: &str) -> &'a str {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
}

fn spotify_artwork(value: &Value) -> Option<String> {
    value
        .pointer("/visualIdentity/image/0/url")
        .or_else(|| value.pointer("/coverArt/sources/0/url"))
        .or_else(|| value.pointer("/album/coverArt/sources/0/url"))
        .or_else(|| value.get("imageUri"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub(crate) fn parse_embed(html: &str, url: &url::Url) -> Result<Collection> {
    let marker = html.find("id=\"__NEXT_DATA__\"").with_context(|| {
        format!(
            "{} (Spotify embed: missing __NEXT_DATA__)",
            tr("Spotify не предоставил открытый список треков")
        )
    })?;
    let script = &html[marker..];
    let start = script.find('>').context(tr("Некорректный ответ Spotify"))? + 1;
    let end = script[start..]
        .find("</script>")
        .context(tr("Некорректный ответ Spotify"))?
        + start;
    let data: Value =
        serde_json::from_str(&script[start..end]).context(tr("Некорректные метаданные Spotify"))?;
    // Spotify's embed error pages can have HTTP 200 and report the actual
    // failure in pageProps.status instead of providing state.data.entity.
    if let Some(status) = data
        .pointer("/props/pageProps/status")
        .and_then(Value::as_u64)
    {
        let message = match status {
            401 | 403 | 429 => tr("Spotify ограничил доступ к открытым метаданным"),
            404 => tr("Коллекция Spotify закрыта или недоступна"),
            500..=599 => tr("Временная ошибка Spotify при получении метаданных"),
            _ => tr("Spotify не предоставил открытый список треков"),
        };
        ensure!(
            status < 400,
            "{message} (Spotify embed: page status={status})"
        );
    }
    let entity = data
        .pointer("/props/pageProps/state/data/entity")
        .filter(|entity| entity.is_object())
        .with_context(|| {
            format!(
                "{} (Spotify embed: missing state.data.entity)",
                tr("Spotify не предоставил открытый список треков")
            )
        })?;
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
        tracks.push(CollectionTrack {
            artwork_url: spotify_artwork(row).or_else(|| spotify_artwork(entity)),
            title: title.into(),
            artist,
            url: track_url,
            duration_ms: row.get("duration").and_then(Value::as_u64),
        });
    }
    ensure!(!tracks.is_empty(), tr("В Spotify нет доступных треков"));
    Ok(Collection {
        artwork_url: spotify_artwork(entity),
        title: text(entity, "title").into(),
        tracks,
        is_collection,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::Arc,
    };

    fn serve_responses(
        statuses: Vec<u16>,
        cancel_after_response: Option<Arc<AtomicBool>>,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!(
            "http://{}/embed/playlist/test",
            listener.local_addr().unwrap()
        );
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            for status in statuses {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(std::time::Instant::now() < deadline, "missing test request");
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                )
                .unwrap();
                if let Some(cancel) = &cancel_after_response {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
        });
        (endpoint, server)
    }

    #[test]
    fn retries_temporary_http_failures_but_limits_attempts() {
        let agent = ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(3)))
            .build()
            .new_agent();
        for statuses in [vec![504, 200], vec![502, 503, 200]] {
            let (endpoint, server) = serve_responses(statuses, None);
            let result = fetch_embed_html(&agent, &endpoint, &AtomicBool::new(false));
            server.join().unwrap();
            assert_eq!(result.unwrap(), "ok");
        }
        let (endpoint, server) = serve_responses(vec![504, 504, 504], None);
        let error = fetch_embed_html(&agent, &endpoint, &AtomicBool::new(false)).unwrap_err();
        server.join().unwrap();
        assert!(error.to_string().contains("HTTP 504; 3 attempts"));

        let (endpoint, server) = serve_responses(vec![404], None);
        let error = fetch_embed_html(&agent, &endpoint, &AtomicBool::new(false)).unwrap_err();
        server.join().unwrap();
        assert!(format!("{error:#}").contains("404"));
    }

    #[test]
    fn cancellation_stops_http_retries() {
        let cancel = Arc::new(AtomicBool::new(false));
        let (endpoint, server) = serve_responses(vec![504], Some(cancel.clone()));
        let agent = ureq::Agent::config_builder()
            .proxy(None)
            .build()
            .new_agent();
        let error = fetch_embed_html(&agent, &endpoint, &cancel).unwrap_err();
        server.join().unwrap();
        assert_eq!(error.to_string(), tr("Загрузка остановлена"));
    }

    #[test]
    fn reports_embed_error_status_instead_of_a_missing_track_list() {
        let url = validate_spotify_url("https://open.spotify.com/playlist/0000000000000000000000")
            .unwrap();
        // Sanitized real Spotify response: the HTTP request succeeds, but the
        // JSON contains a 404 error page and no entity.
        let unavailable = include_str!("../tests/fixtures/spotify-unavailable.json");
        let error = parse_embed(
            &format!("<script id=\"__NEXT_DATA__\">{unavailable}</script>"),
            &url,
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("page status=404"));
        assert!(
            error
                .to_string()
                .contains(tr("Коллекция Spotify закрыта или недоступна"))
        );

        for (status, message) in [
            (403, "Spotify ограничил доступ к открытым метаданным"),
            (429, "Spotify ограничил доступ к открытым метаданным"),
            (503, "Временная ошибка Spotify при получении метаданных"),
        ] {
            let data = serde_json::json!({"props":{"pageProps":{"status":status}}});
            let error = parse_embed(
                &format!("<script id=\"__NEXT_DATA__\">{data}</script>"),
                &url,
            )
            .err()
            .unwrap();
            assert!(error.to_string().contains(tr(message)));
            assert!(error.to_string().contains(&format!("page status={status}")));
        }
    }

    #[test]
    fn distinguishes_missing_metadata_without_exposing_response_contents() {
        let url = validate_spotify_url("https://open.spotify.com/playlist/0000000000000000000000")
            .unwrap();
        for (html, reason) in [
            (
                "<html>untrusted response contents</html>",
                "missing __NEXT_DATA__",
            ),
            (
                "<script id=\"__NEXT_DATA__\">{\"props\":{\"pageProps\":{\"secret\":\"untrusted response contents\"}}}</script>",
                "missing state.data.entity",
            ),
            (
                "<script id=\"__NEXT_DATA__\">{\"props\":{\"pageProps\":{\"state\":{\"data\":{\"entity\":null}}}}}</script>",
                "missing state.data.entity",
            ),
        ] {
            let error = parse_embed(html, &url).err().unwrap().to_string();
            assert!(error.contains(reason), "{error}");
            assert!(!error.contains("untrusted response contents"));
        }
    }

    #[test]
    #[ignore = "requires public Spotify network access"]
    fn fetches_reported_public_playlist() {
        let url = validate_spotify_url("https://open.spotify.com/playlist/3g3ZXjjQgq3NGT8gI1QhqV")
            .unwrap();
        let collection = fetch(&url, &AtomicBool::new(false)).unwrap();
        assert!(collection.is_collection);
        assert!(!collection.tracks.is_empty());
        println!(
            "Spotify reported playlist: {} tracks",
            collection.tracks.len()
        );
    }

    #[test]
    fn extracts_album_artwork_and_individual_track_covers() {
        let url =
            validate_spotify_url("https://open.spotify.com/album/5dABmDwage8L9rZ4SFpMVM").unwrap();
        let entity = serde_json::json!({"uri":"spotify:album:5dABmDwage8L9rZ4SFpMVM","title":"Album",
            "coverArt":{"sources":[{"url":"https://i.scdn.co/image/album"}]},
            "trackList":[{"uri":"spotify:track:3LuVARmTf1iY1EsLFIDHo2","title":"Track","subtitle":"Artist",
                "coverArt":{"sources":[{"url":"https://i.scdn.co/image/track"}]}}]});
        let data = serde_json::json!({"props":{"pageProps":{"state":{"data":{"entity":entity}}}}});
        let collection = parse_embed(
            &format!("<script id=\"__NEXT_DATA__\">{data}</script>"),
            &url,
        )
        .unwrap();
        assert_eq!(
            collection.artwork_url.as_deref(),
            Some("https://i.scdn.co/image/album")
        );
        assert_eq!(
            collection.tracks[0].artwork_url.as_deref(),
            Some("https://i.scdn.co/image/track")
        );
    }

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
