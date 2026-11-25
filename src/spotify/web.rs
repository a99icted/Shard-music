//! Anonymous web-player metadata, using the same paged queries as SpotAPI/spotDL.
//! Query hashes come from Spotify's current bundle rather than a pinned build.
use super::{fetch_embed_html, spotify_artwork, text, validate_spotify_url};
use crate::{
    i18n::tr,
    music_metadata::{Collection, CollectionTrack},
};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha1::Sha1;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const PAGE_SIZE: usize = 100;
const MAX_ITEMS: usize = 100_000;
// Public web-player TOTP versions maintained by the source also used by SpotAPI.
const SECRETS_URL: &str =
    "https://code.thetadev.de/ThetaDev/spotify-secrets/raw/branch/main/secrets/secretDict.json";

pub(super) fn fetch(
    agent: &ureq::Agent,
    url: &url::Url,
    cancel: &AtomicBool,
) -> Result<Collection> {
    fetch_with_page_size(agent, url, cancel, PAGE_SIZE)
}

fn fetch_with_page_size(
    agent: &ureq::Agent,
    url: &url::Url,
    cancel: &AtomicBool,
    page_size: usize,
) -> Result<Collection> {
    let playlist = url.path().starts_with("/playlist/");
    let operation = if playlist {
        "fetchPlaylist"
    } else {
        "getAlbum"
    };
    let html = fetch_embed_html(agent, url.as_str(), cancel)?;
    let files = script_json(&html, "__CDN_FILE_URLS__")?;
    let bundle = files
        .as_object()
        .and_then(|files| {
            files.iter().find_map(|(name, value)| {
                (name.starts_with("build/web-player/web-player.") && name.ends_with(".js"))
                    .then(|| value.as_str())
                    .flatten()
            })
        })
        .context(tr("Некорректные метаданные Spotify"))?;
    let bundle_url = url::Url::parse(bundle)?;
    ensure!(
        bundle_url.scheme() == "https" && bundle_url.host_str() == Some("open.spotifycdn.com"),
        tr("Некорректный ответ Spotify")
    );
    let js = fetch_embed_html(agent, bundle, cancel)?;
    let hash = query_hash(&js, operation)?;
    let token = access_token(agent, cancel)?;
    let uri = format!(
        "spotify:{}",
        url.path().trim_start_matches('/').replace('/', ":")
    );
    collect_pages(url, |offset| {
        ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
        let variables = if playlist {
            json!({"uri":uri,"offset":offset,"limit":page_size,"enableWatchFeedEntrypoint":false})
        } else {
            json!({"uri":uri,"offset":offset,"limit":page_size,"locale":""})
        };
        query(agent, operation, &hash, &token, variables, cancel)
    })
}

fn script_json(html: &str, id: &str) -> Result<Value> {
    let marker = format!("id=\"{id}\"");
    let script = html
        .find(&marker)
        .map(|i| &html[i..])
        .context(tr("Некорректный ответ Spotify"))?;
    let start = script.find('>').context(tr("Некорректный ответ Spotify"))? + 1;
    let end = script[start..]
        .find("</script>")
        .context(tr("Некорректный ответ Spotify"))?
        + start;
    let decoded = STANDARD
        .decode(script[start..end].trim())
        .context(tr("Некорректные метаданные Spotify"))?;
    serde_json::from_slice(&decoded).context(tr("Некорректные метаданные Spotify"))
}

fn query_hash(js: &str, operation: &str) -> Result<String> {
    let marker = format!("\"{operation}\",\"query\",\"");
    let hash = js
        .split_once(&marker)
        .and_then(|(_, rest)| rest.split('"').next())
        .context(tr("Некорректные метаданные Spotify"))?;
    ensure!(
        hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
        tr("Некорректные метаданные Spotify")
    );
    Ok(hash.to_owned())
}

fn totp(secret: &[u8], seconds: u64) -> Result<String> {
    // Spotify transforms the public secret before applying RFC 6238 (SHA-1).
    let key: String = secret
        .iter()
        .enumerate()
        .map(|(i, b)| (b ^ ((i % 33) as u8 + 9)).to_string())
        .collect();
    let mut mac = Hmac::<Sha1>::new_from_slice(key.as_bytes())?;
    mac.update(&(seconds / 30).to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[19] & 15) as usize;
    let value = u32::from_be_bytes(digest[offset..offset + 4].try_into()?) & 0x7fff_ffff;
    Ok(format!("{:06}", value % 1_000_000))
}

fn access_token(agent: &ureq::Agent, cancel: &AtomicBool) -> Result<String> {
    let secrets: Value = serde_json::from_str(&fetch_embed_html(agent, SECRETS_URL, cancel)?)
        .context(tr("Некорректные метаданные Spotify"))?;
    let (version, secret) = secrets
        .as_object()
        .and_then(|versions| {
            versions
                .iter()
                .filter_map(|(version, secret)| Some((version.parse::<u64>().ok()?, secret)))
                .max_by_key(|(version, _)| *version)
        })
        .context(tr("Некорректные метаданные Spotify"))?;
    let secret = secret
        .as_array()
        .context(tr("Некорректные метаданные Spotify"))?
        .iter()
        .map(|n| {
            n.as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .context(tr("Некорректные метаданные Spotify"))
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        !secret.is_empty() && secret.len() <= 1024,
        tr("Некорректные метаданные Spotify")
    );
    let code = totp(
        &secret,
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    )?;
    let mut endpoint = url::Url::parse("https://open.spotify.com/api/token")?;
    endpoint.query_pairs_mut().extend_pairs([
        ("reason", "init"),
        ("productType", "web-player"),
        ("totp", &code),
        ("totpServer", &code),
        ("totpVer", &version.to_string()),
    ]);
    let data: Value = serde_json::from_str(&fetch_embed_html(agent, endpoint.as_str(), cancel)?)
        .context(tr("Некорректные метаданные Spotify"))?;
    data.get("accessToken")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .context(tr("Spotify ограничил доступ к открытым метаданным"))
}

fn query(
    agent: &ureq::Agent,
    operation: &str,
    hash: &str,
    token: &str,
    variables: Value,
    cancel: &AtomicBool,
) -> Result<Value> {
    let mut endpoint = url::Url::parse("https://api-partner.spotify.com/pathfinder/v1/query")?;
    endpoint.query_pairs_mut().extend_pairs([
        ("operationName", operation),
        ("variables", &variables.to_string()),
        (
            "extensions",
            &json!({"persistedQuery":{"version":1,"sha256Hash":hash}}).to_string(),
        ),
    ]);
    for attempt in 0..3 {
        ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
        let response = agent
            .post(endpoint.as_str())
            .header("User-Agent", "Mozilla/5.0")
            .header("Authorization", &format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .send_empty();
        let mut response = match response {
            Err(ureq::Error::StatusCode(502 | 503 | 504)) if attempt < 2 => continue,
            response => response.context(tr("Не удалось получить открытые метаданные Spotify"))?,
        };
        let body = response
            .body_mut()
            .with_config()
            .limit(16 * 1024 * 1024)
            .read_to_string()?;
        ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
        let data: Value =
            serde_json::from_str(&body).context(tr("Некорректные метаданные Spotify"))?;
        ensure!(
            data.get("errors")
                .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)),
            tr("Spotify не предоставил открытый список треков")
        );
        return Ok(data);
    }
    unreachable!()
}

fn collect_pages(
    url: &url::Url,
    mut page: impl FnMut(usize) -> Result<Value>,
) -> Result<Collection> {
    let playlist = url.path().starts_with("/playlist/");
    let uri = format!(
        "spotify:{}",
        url.path().trim_start_matches('/').replace('/', ":")
    );
    let mut collection = Collection {
        artwork_url: None,
        title: String::new(),
        tracks: Vec::new(),
        is_collection: true,
        use_collection_artwork: !playlist,
    };
    let mut offset = 0;
    let mut expected = None;
    loop {
        let data = page(offset)?;
        let entity = data
            .pointer(if playlist {
                "/data/playlistV2"
            } else {
                "/data/albumUnion"
            })
            .context(tr("Коллекция Spotify закрыта или недоступна"))?;
        ensure!(
            text(entity, "uri") == uri,
            tr("Spotify вернул другую коллекцию")
        );
        let content = entity
            .get(if playlist { "content" } else { "tracksV2" })
            .context(tr("Некорректные метаданные Spotify"))?;
        let total = content
            .get("totalCount")
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .context(tr("Некорректные метаданные Spotify"))?;
        ensure!(
            total <= MAX_ITEMS && expected.is_none_or(|previous| previous == total),
            tr("Spotify не предоставил полный список треков")
        );
        let rows = content
            .get("items")
            .and_then(Value::as_array)
            .context(tr("Некорректные метаданные Spotify"))?;
        if let Some(page_offset) = content
            .pointer("/pagingInfo/offset")
            .and_then(Value::as_u64)
        {
            ensure!(
                page_offset == offset as u64,
                tr("Spotify не предоставил полный список треков")
            );
        }
        ensure!(
            offset + rows.len() <= total && (!rows.is_empty() || offset == total),
            tr("Spotify не предоставил полный список треков")
        );
        if offset == 0 {
            collection.title = text(entity, "name").to_owned();
            collection.artwork_url = web_artwork(entity);
        }
        expected = Some(total);
        for row in rows {
            let track = if playlist {
                row.pointer("/itemV2/data")
            } else {
                row.get("track")
            }
            .context(tr("Некорректные метаданные Spotify"))?;
            // Restricted/local items still count toward the page offset, even
            // though Spotify does not expose downloadable track metadata.
            if let Some(track) = parse_track(
                track,
                if playlist {
                    None
                } else {
                    collection.artwork_url.clone()
                },
            )? {
                collection.tracks.push(track);
            }
        }
        offset += rows.len();
        if offset == total {
            break;
        }
    }
    ensure!(
        !collection.tracks.is_empty(),
        tr("В Spotify нет доступных треков")
    );
    Ok(collection)
}

fn web_artwork(value: &Value) -> Option<String> {
    let sources = value
        .pointer("/albumOfTrack/coverArt/sources")
        .or_else(|| value.pointer("/coverArt/sources"))
        .or_else(|| value.pointer("/images/items/0/sources"));
    sources
        .and_then(Value::as_array)
        .and_then(|sources| {
            sources
                .iter()
                .filter(|v| {
                    v.get("url")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.trim().is_empty())
                })
                .max_by_key(|v| v.get("width").and_then(Value::as_u64).unwrap_or(0))
        })
        .and_then(|v| v.get("url"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| spotify_artwork(value))
}

fn parse_track(track: &Value, album_artwork: Option<String>) -> Result<Option<CollectionTrack>> {
    let Some(id) = text(track, "uri").strip_prefix("spotify:track:") else {
        return Ok(None);
    };
    let title = text(track, "name");
    ensure!(!title.is_empty(), tr("Spotify вернул трек без названия"));
    let artists = track
        .pointer("/artists/items")
        .and_then(Value::as_array)
        .context(tr("Spotify вернул трек без исполнителя"))?;
    let artist = artists
        .iter()
        .map(|artist| artist.get("profile").unwrap_or(artist))
        .map(|artist| text(artist, "name"))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    ensure!(
        !artist.is_empty(),
        tr("Spotify вернул трек без исполнителя")
    );
    Ok(Some(CollectionTrack {
        artwork_url: web_artwork(track).or(album_artwork),
        title: title.to_owned(),
        artist,
        url: validate_spotify_url(&format!("https://open.spotify.com/track/{id}"))?,
        duration_ms: track
            .pointer("/duration/totalMilliseconds")
            .and_then(Value::as_u64)
            .or_else(|| {
                track
                    .pointer("/trackDuration/totalMilliseconds")
                    .and_then(Value::as_u64)
            }),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(kind: &str) -> url::Url {
        validate_spotify_url(&format!(
            "https://open.spotify.com/{kind}/3g3ZXjjQgq3NGT8gI1QhqV"
        ))
        .unwrap()
    }

    fn track(name: &str, artwork: bool) -> Value {
        let mut track = json!({"uri":"spotify:track:4uLU6hMCjMI75M1A2tKUQC", "name":name,
            "artists":{"items":[{"profile":{"name":"Artist"}},{"profile":{"name":"Other"}}]},
            "trackDuration":{"totalMilliseconds":123000}});
        if artwork {
            track["albumOfTrack"] = json!({"coverArt":{"sources":[
            {"url":"https://example.com/small.jpg","width":64},
            {"url":"https://example.com/song.jpg","width":640}]}});
        }
        track
    }

    fn page(playlist: bool, offset: usize, total: usize, tracks: Vec<Value>) -> Value {
        let items: Vec<_> = tracks
            .into_iter()
            .map(|track| {
                if playlist {
                    json!({"itemV2":{"data":track}})
                } else {
                    json!({"track":track})
                }
            })
            .collect();
        let kind = if playlist { "playlist" } else { "album" };
        let mut entity = json!({"uri":format!("spotify:{kind}:3g3ZXjjQgq3NGT8gI1QhqV"), "name":"Collection",
            "coverArt":{"sources":[{"url":"https://example.com/collection.jpg","width":640}]}});
        entity[if playlist { "content" } else { "tracksV2" }] =
            json!({"items":items,"totalCount":total,"pagingInfo":{"offset":offset}});
        if playlist {
            json!({"data":{"playlistV2":entity}})
        } else {
            json!({"data":{"albumUnion":entity}})
        }
    }

    #[test]
    fn paginates_entire_playlist_counting_restricted_rows_and_preserving_duplicates() {
        let mut offsets = vec![];
        let collection = collect_pages(&url("playlist"), |offset| {
            offsets.push(offset);
            Ok(match offset {
                0 => page(
                    true,
                    0,
                    509,
                    (0..100)
                        .map(|i| {
                            if i == 3 {
                                json!({"__typename":"RestrictedContent"})
                            } else {
                                track(&i.to_string(), true)
                            }
                        })
                        .collect(),
                ),
                offset => page(
                    true,
                    offset,
                    509,
                    (offset..(offset + 100).min(509))
                        .map(|i| track(&i.to_string(), false))
                        .collect(),
                ),
            })
        })
        .unwrap();
        assert_eq!(offsets, [0, 100, 200, 300, 400, 500]);
        assert_eq!(collection.tracks.len(), 508);
        assert_eq!(collection.tracks[3].title, "4");
        assert_eq!(collection.tracks.last().unwrap().title, "508");
        assert_eq!(
            collection.tracks[0].artwork_url.as_deref(),
            Some("https://example.com/song.jpg")
        );
        assert!(collection.tracks[100].artwork_url.is_none());
        assert!(!collection.use_collection_artwork);
        assert!(collection.artwork_url.is_some());
        assert_eq!(collection.tracks[0].artist, "Artist, Other");
        assert_eq!(collection.tracks[0].duration_ms, Some(123000));
    }

    #[test]
    fn paginates_albums_and_retains_album_artwork() {
        let mut offsets = vec![];
        let collection = collect_pages(&url("album"), |offset| {
            offsets.push(offset);
            Ok(page(
                false,
                offset,
                3,
                (offset..(offset + 2).min(3))
                    .map(|i| track(&i.to_string(), i == 0))
                    .collect(),
            ))
        })
        .unwrap();
        assert_eq!(offsets, [0, 2]);
        assert_eq!(collection.tracks.len(), 3);
        assert!(collection.use_collection_artwork);
        assert_eq!(
            collection.tracks[0].artwork_url.as_deref(),
            Some("https://example.com/song.jpg")
        );
        assert_eq!(collection.tracks[1].artwork_url, collection.artwork_url);
        assert_eq!(collection.tracks[2].title, "2");
    }

    #[test]
    fn never_returns_partial_success_after_missing_pages_or_changed_totals() {
        for failure in 0..5 {
            let result = collect_pages(&url("playlist"), |offset| {
                if offset == 0 {
                    return Ok(page(true, 0, 3, vec![track("first", false)]));
                }
                match failure {
                    0 => anyhow::bail!("network failure on next page"),
                    1 => Ok(page(true, 1, 3, vec![])),
                    2 => Ok(page(true, 1, 2, vec![track("changed", false)])),
                    3 => Ok(page(true, 0, 3, vec![track("repeated page", false)])),
                    _ => Ok(json!({"data":{"playlistV2":null}})),
                }
            });
            assert!(result.is_err(), "failure {failure}");
        }
    }

    #[test]
    fn extracts_current_bundle_hashes_and_generates_spotify_totp() {
        let hash = "a".repeat(64);
        assert_eq!(
            query_hash(
                &format!("x(\"fetchPlaylist\",\"query\",\"{hash}\")"),
                "fetchPlaylist"
            )
            .unwrap(),
            hash
        );
        assert!(query_hash("untrusted response", "fetchPlaylist").is_err());
        let secret = [
            44, 55, 47, 42, 70, 40, 34, 114, 76, 74, 50, 111, 120, 97, 75, 76, 94, 102, 43, 69, 49,
            120, 118, 80, 64, 78,
        ];
        assert_eq!(totp(&secret, 59).unwrap(), "332823");
        assert_eq!(totp(&secret, 1_700_000_000).unwrap(), "371599");
    }

    #[test]
    #[ignore = "requires public Spotify network access"]
    fn fetches_complete_public_playlist_and_paged_albums() {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(25)))
            .build()
            .new_agent();
        let cancel = AtomicBool::new(false);
        let playlist = fetch(&agent, &url("playlist"), &cancel).unwrap();
        println!(
            "Reported playlist: {} available tracks",
            playlist.tracks.len()
        );
        assert!(playlist.tracks.len() > 490);
        assert!(!playlist.use_collection_artwork);
        assert!(
            playlist
                .tracks
                .iter()
                .filter(|track| track.artwork_url.is_some())
                .count()
                > 400
        );
        for (id, expected) in [
            ("5dABmDwage8L9rZ4SFpMVM", 8),
            ("5H7ixXZfsNMGbIE5OBSpcb", 31),
        ] {
            // Small pages exercise album pagination even on ordinary releases.
            let url =
                validate_spotify_url(&format!("https://open.spotify.com/album/{id}")).unwrap();
            let album = fetch_with_page_size(&agent, &url, &cancel, 7).unwrap();
            println!("Album {}: {} tracks", album.title, album.tracks.len());
            assert_eq!(album.tracks.len(), expected);
            assert!(album.use_collection_artwork);
            assert!(album.artwork_url.is_some());
            assert!(album.tracks.iter().all(|track| track.artwork_url.is_some()));
            let bytes = crate::artwork::download(album.artwork_url.as_deref().unwrap()).unwrap();
            assert!(image::load_from_memory(&bytes).is_ok());
        }
    }
}
