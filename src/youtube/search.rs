//! Native backend for YouTube discovery. No browser requests or downloaded audio
//! are needed for suggestions, search, or inspecting a collection.
use super::*;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultKind {
    Track,
    Album,
    Playlist,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub url: String,
    pub thumbnail: Option<String>,
    pub duration_ms: Option<i64>,
    pub play_count: Option<String>,
    pub kind: ResultKind,
}

/// Snapshot the expanded collection so a queued download keeps its name,
/// artwork and track order even when the user changes the search query.
#[derive(Debug, Clone)]
pub struct CollectionDownload {
    pub collection: SearchResult,
    pub tracks: Vec<SearchResult>,
}

pub(crate) struct CollectionSource<'a, S> {
    pub source: &'a S,
    pub collection: &'a SearchResult,
}
impl<S: YoutubeSource> YoutubeSource for CollectionSource<'_, S> {
    fn artwork_url(&self) -> Option<&str> {
        self.collection.thumbnail.as_deref()
    }
    fn playlist(&self, url: &url::Url, cancel: &AtomicBool) -> Result<YoutubePlaylist> {
        self.source.playlist(url, cancel)
    }
    fn track(
        &self,
        url: &url::Url,
        staging: &Path,
        cancel: &AtomicBool,
    ) -> Result<DownloadedAudio> {
        self.source.track(url, staging, cancel)
    }
}

fn agent(suggestions: bool) -> &'static ureq::Agent {
    // Agents own the DNS cache and connection pool. Rebuilding one per keystroke
    // paid for DNS/TCP/TLS again, even when querying the same host.
    static SUGGESTIONS: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    static MUSIC: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    let cache = if suggestions { &SUGGESTIONS } else { &MUSIC };
    cache.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(if suggestions { 4 } else { 10 })))
            .timeout_connect(Some(Duration::from_secs(3)))
            .build()
            .into()
    })
}

/// Proxy Google's YouTube suggestions through the native backend.
pub fn suggestions(query: &str) -> Result<Vec<String>> {
    let mut url = url::Url::parse("https://suggestqueries.google.com/complete/search")?;
    url.query_pairs_mut()
        .extend_pairs([("client", "firefox"), ("ds", "yt"), ("q", query)]);
    let body = agent(true)
        .get(url.as_str())
        .call()?
        .body_mut()
        .read_to_string()?;
    parse_suggestions(&body)
}

fn parse_suggestions(body: &str) -> Result<Vec<String>> {
    let value: Value = serde_json::from_str(body)?;
    let items = value[1].as_array().context("Invalid YouTube suggestions")?;
    let mut seen = HashSet::new();
    Ok(items
        .iter()
        .filter_map(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .filter(|s| seen.insert(s.to_string()))
        .take(10)
        .map(str::to_owned)
        .collect())
}

fn music_request(endpoint: &str, fields: Value) -> Result<Value> {
    let mut body = fields;
    body["context"] = json!({"client": {"clientName": "WEB_REMIX",
        "clientVersion": "1.20241125.01.00", "hl": "en", "gl": "US"}});
    let text = agent(false)
        .post(format!(
            "https://music.youtube.com/youtubei/v1/{endpoint}?prettyPrint=false"
        ))
        .header("Content-Type", "application/json")
        .header("User-Agent", "Mozilla/5.0")
        .send(body.to_string())?
        .body_mut()
        .read_to_string()?;
    let value: Value = serde_json::from_str(&text)?;
    ensure!(
        value.get("error").is_none(),
        "YouTube Music: {}",
        value["error"]
    );
    Ok(value)
}

/// YouTube Music exposes one category per request. Combinations use parallel
/// category requests. Ordinary video search is a fallback when Music finds no tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchFilters {
    pub tracks: bool,
    pub albums: bool,
    pub playlists: bool,
}
impl Default for SearchFilters {
    fn default() -> Self {
        Self {
            tracks: true,
            albums: true,
            playlists: true,
        }
    }
}
impl SearchFilters {
    pub fn allows(self, kind: ResultKind) -> bool {
        match kind {
            ResultKind::Track => self.tracks,
            ResultKind::Album => self.albums,
            ResultKind::Playlist => self.playlists,
        }
    }
    pub fn mask(self) -> u8 {
        u8::from(self.tracks) | u8::from(self.albums) << 1 | u8::from(self.playlists) << 2
    }
    pub fn is_empty(self) -> bool {
        self.mask() == 0
    }
    pub fn cache_key(self, query: &str) -> String {
        format!("{}:{query}", self.mask())
    }
}

// Parameter values used by ytmusicapi's get_search_params (public Music API).
// https://github.com/sigma67/ytmusicapi/blob/main/ytmusicapi/parsers/search.py
fn category_params(kind: ResultKind) -> &'static str {
    match kind {
        ResultKind::Track => "EgWKAQIIAWoMEA4QChADEAQQCRAF",
        ResultKind::Album => "EgWKAQIYAWoMEA4QChADEAQQCRAF",
        ResultKind::Playlist => "Eg-KAQwIABAAGAAgACgBMABqChAEEAMQCRAFEAo%3D",
    }
}

pub fn search(root: &Path, query: &str, cancel: &AtomicBool) -> Result<Vec<SearchResult>> {
    search_filtered(root, query, SearchFilters::default(), cancel)
}

pub fn search_filtered(
    root: &Path,
    query: &str,
    filters: SearchFilters,
    cancel: &AtomicBool,
) -> Result<Vec<SearchResult>> {
    ensure!(!cancel.load(Ordering::Relaxed), "Search cancelled");
    if filters.is_empty() {
        return Ok(vec![]);
    }
    ensure!(!query.trim().is_empty(), "Empty search query");
    // Mixed Music responses sometimes omit an entire selected category or song
    // metadata. Fetch selected categories in parallel for complete, stable results.
    let categories: Vec<_> = [ResultKind::Track, ResultKind::Album, ResultKind::Playlist]
        .into_iter()
        .filter(|kind| filters.allows(*kind))
        .collect();
    let responses = std::thread::scope(|scope| {
        let mut jobs = Vec::new();
        for category in categories {
            jobs.push(scope.spawn(move || {
                ensure!(!cancel.load(Ordering::Relaxed), "Search cancelled");
                let body = json!({"query": query, "params": category_params(category)});
                let mut results = parse_music(&music_request("search", body)?);
                results.retain(|r| r.kind == category);
                Ok(results)
            }));
        }
        jobs.into_iter()
            .map(|job| {
                job.join()
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("YouTube search failed")))
            })
            .collect::<Vec<Result<Vec<SearchResult>>>>()
    });
    ensure!(!cancel.load(Ordering::Relaxed), "Search cancelled");
    let results = complete_search(responses, filters, || {
        ensure!(!cancel.load(Ordering::Relaxed), "Search cancelled");
        let client = YoutubeClient::new(root)?;
        let mut command = client.command();
        command
            .args([
                "--flat-playlist",
                "--dump-single-json",
                "--skip-download",
                "--",
            ])
            .arg(format!("ytsearch20:{query}"));
        let output = run_command(command, cancel, Duration::from_secs(20))?;
        ensure!(output.success, "yt-dlp: {}", output.error());
        parse_flat(&output.stdout)
    });
    ensure!(!cancel.load(Ordering::Relaxed), "Search cancelled");
    results
}

fn complete_search(
    mut responses: Vec<Result<Vec<SearchResult>>>,
    filters: SearchFilters,
    fallback: impl FnOnce() -> Result<Vec<SearchResult>>,
) -> Result<Vec<SearchResult>> {
    // Return native API results without waiting for a Python process, tool
    // discovery or a second search. Keep video search for queries Music cannot answer.
    if filters.tracks
        && !responses.iter().any(|response| {
            response
                .as_ref()
                .is_ok_and(|items| items.iter().any(|r| r.kind == ResultKind::Track))
        })
    {
        responses.push(fallback());
    }
    let mut results = Vec::new();
    let mut success = false;
    let mut error = None;
    for response in responses {
        match response {
            Ok(items) => {
                success = true;
                results.extend(items.into_iter().filter(|r| filters.allows(r.kind)));
            }
            Err(failure) => {
                error = Some(failure);
            }
        }
    }
    if !success {
        return Err(error.unwrap_or_else(|| anyhow::anyhow!("No search sources")));
    }
    let mut positions = HashMap::<String, usize>::new();
    let mut merged: Vec<SearchResult> = Vec::new();
    for result in results.drain(..) {
        if let Some(index) = positions.get(&result.id).copied() {
            let existing = &mut merged[index];
            if existing.artist.is_empty() {
                existing.artist = result.artist;
            }
            existing.duration_ms = existing.duration_ms.or(result.duration_ms);
            existing.play_count = existing.play_count.take().or(result.play_count);
            existing.thumbnail = existing.thumbnail.take().or(result.thumbnail);
        } else {
            positions.insert(result.id.clone(), merged.len());
            merged.push(result);
        }
    }
    Ok(merged)
}

pub fn collection(
    root: &Path,
    result: &SearchResult,
    cancel: &AtomicBool,
) -> Result<Vec<SearchResult>> {
    let target = collection_url(result)?;
    let client = YoutubeClient::new(root)?;
    let mut command = client.command();
    command
        .args([
            "--yes-playlist",
            "--flat-playlist",
            "--dump-single-json",
            "--skip-download",
            "--ignore-errors",
            "--",
        ])
        .arg(target);
    let output = run_command(command, cancel, Duration::from_secs(120))?;
    ensure!(output.success, "yt-dlp: {}", output.error());
    parse_flat(&output.stdout)
}

pub fn collection_url(result: &SearchResult) -> Result<String> {
    if result.kind != ResultKind::Album || !result.id.starts_with("MPRE") {
        return Ok(result.url.clone());
    }
    let value = music_request("browse", json!({"browseId": result.id}))?;
    album_playlist_url(&value)
}

fn album_playlist_url(value: &Value) -> Result<String> {
    // Albums have a canonical playlist URL in the microformat; the play button
    // also supplies the audio playlist ID on newer Music responses.
    let mut candidates = Vec::new();
    walk(value, &mut |v| {
        if let Some(url) = v["microformatDataRenderer"]["urlCanonical"].as_str() {
            if let Ok(url) = validate_youtube_url(url) {
                if let Some(url) = playlist_url(&url) {
                    candidates.push((3, url.to_string()));
                }
            }
        }
        if let Some(id) = v["audioPlaylistId"].as_str() {
            candidates.push((2, format!("https://www.youtube.com/playlist?list={id}")));
        }
        for key in ["watchPlaylistEndpoint", "watchEndpoint"] {
            if let Some(id) = v[key]["playlistId"]
                .as_str()
                .filter(|id| id.starts_with("OLAK5uy"))
            {
                candidates.push((1, format!("https://www.youtube.com/playlist?list={id}")));
            }
        }
    });
    candidates
        .into_iter()
        .max_by_key(|c| c.0)
        .map(|c| c.1)
        .context("YouTube did not return the album playlist")
}

#[derive(Clone, Debug)]
pub struct StreamInfo {
    pub url: String,
    pub artist: Option<String>,
    pub duration_ms: Option<i64>,
    pub play_count: Option<String>,
}

pub fn stream_url(root: &Path, result: &SearchResult, cancel: &AtomicBool) -> Result<String> {
    Ok(resolve_stream(root, result, cancel)?.url)
}

pub fn resolve_stream(
    root: &Path,
    result: &SearchResult,
    cancel: &AtomicBool,
) -> Result<StreamInfo> {
    ensure!(result.kind == ResultKind::Track, "Select a track to play");
    let client = YoutubeClient::new(root)?;
    let mut command = client.command();
    command
        .args([
            "--no-playlist",
            "--format",
            "bestaudio[ext=m4a]/bestaudio",
            "--dump-single-json",
            "--skip-download",
            "--",
        ])
        .arg(&result.url);
    let output = run_command(command, cancel, Duration::from_secs(90))?;
    ensure!(output.success, "yt-dlp: {}", output.error());
    let value: Value = serde_json::from_slice(&output.stdout)?;
    let url = url::Url::parse(value["url"].as_str().context("No stream URL")?)?;
    ensure!(url.scheme() == "https", "Invalid stream URL");
    Ok(StreamInfo {
        url: url.into(),
        artist: value["artist"]
            .as_str()
            .or(value["creator"].as_str())
            .or(value["uploader"].as_str())
            .map(|s| s.trim_end_matches(" - Topic").to_owned())
            .filter(|s| !s.is_empty()),
        duration_ms: value["duration"]
            .as_f64()
            .filter(|d| d.is_finite() && *d > 0.)
            .map(|d| (d * 1000.) as i64)
            .or(result.duration_ms),
        // Music play counts can aggregate releases. Keep that value rather
        // than replacing it with the watch page's different view counter.
        play_count: result
            .play_count
            .clone()
            .or_else(|| value["view_count"].as_u64().map(|n| n.to_string())),
    })
}

fn walk(value: &Value, visit: &mut impl FnMut(&Value)) {
    visit(value);
    match value {
        Value::Object(map) => {
            for v in map.values() {
                walk(v, visit);
            }
        }
        Value::Array(items) => {
            for v in items {
                walk(v, visit);
            }
        }
        _ => {}
    }
}
fn text(value: &Value) -> String {
    value["runs"]
        .as_array()
        .map(|runs| runs.iter().filter_map(|r| r["text"].as_str()).collect())
        .or_else(|| value["simpleText"].as_str().map(str::to_owned))
        .unwrap_or_default()
}
fn thumbnail(value: &Value) -> Option<String> {
    let mut found = None;
    walk(value, &mut |v| {
        if let Some(items) = v["thumbnails"].as_array() {
            found = items
                .iter()
                .rev()
                .filter_map(|v| v["url"].as_str())
                .find(|url| url.starts_with("https://"))
                .map(str::to_owned)
                .or(found.take());
        }
    });
    found
}
fn duration(text: &str) -> Option<i64> {
    let parts = text
        .split(':')
        .map(str::parse::<i64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
    (parts.len() >= 2 && parts.len() <= 3 && parts.iter().all(|p| *p >= 0))
        .then(|| parts.iter().fold(0, |a, p| a * 60 + p).saturating_mul(1000))
}

fn parse_music(value: &Value) -> Vec<SearchResult> {
    let mut results = Vec::new();
    walk(value, &mut |v| {
        let Some(item) = v
            .get("musicResponsiveListItemRenderer")
            .or_else(|| v.get("musicTwoRowItemRenderer"))
        else {
            return;
        };
        let title = if item.get("title").is_some() {
            text(&item["title"])
        } else {
            text(&item["flexColumns"][0]["musicResponsiveListItemFlexColumnRenderer"]["text"])
        };
        if title.is_empty() {
            return;
        }
        let mut video = None;
        let mut browse = None;
        let mut page_type = String::new();
        // Prefer the item's navigation, not the artist/album links in its subtitle.
        let endpoint = &item["navigationEndpoint"];
        if let Some(id) = endpoint["browseEndpoint"]["browseId"].as_str() {
            browse = Some(id.to_owned());
            page_type = endpoint["browseEndpoint"]["browseEndpointContextSupportedConfigs"]["browseEndpointContextMusicConfig"]["pageType"].as_str().unwrap_or_default().into();
        }
        walk(&item["overlay"], &mut |v| {
            if let Some(id) = v["watchEndpoint"]["videoId"].as_str() {
                video = Some(id.to_owned());
            }
        });
        if video.is_none() {
            video = item["playlistItemData"]["videoId"]
                .as_str()
                .map(str::to_owned);
        }
        if video.is_none() {
            video = endpoint["watchEndpoint"]["videoId"]
                .as_str()
                .map(str::to_owned);
        }
        let (id, kind, url) =
            if let Some(id) = browse.filter(|id| id.starts_with("MPRE") || id.starts_with("VL")) {
                if id.starts_with("MPRE") || page_type.contains("ALBUM") {
                    (
                        id.clone(),
                        ResultKind::Album,
                        format!("https://music.youtube.com/browse/{id}"),
                    )
                } else {
                    (
                        id.clone(),
                        ResultKind::Playlist,
                        format!(
                            "https://www.youtube.com/playlist?list={}",
                            id.trim_start_matches("VL")
                        ),
                    )
                }
            } else if let Some(id) = video {
                (
                    id.clone(),
                    ResultKind::Track,
                    format!("https://www.youtube.com/watch?v={id}"),
                )
            } else {
                return;
            };
        let subtitle = if item.get("subtitle").is_some() {
            &item["subtitle"]
        } else {
            &item["flexColumns"][1]["musicResponsiveListItemFlexColumnRenderer"]["text"]
        };
        let artist = subtitle["runs"]
            .as_array()
            .and_then(|runs| {
                runs.iter().find(|r| {
                    r["navigationEndpoint"]["browseEndpoint"]["browseId"]
                        .as_str()
                        .is_some_and(|id| id.starts_with("UC"))
                })
            })
            .and_then(|r| r["text"].as_str())
            .unwrap_or_default()
            .to_owned();
        let mut duration_ms = None;
        let mut play_count = None;
        for column in ["subtitle", "flexColumns", "fixedColumns"] {
            walk(&item[column], &mut |v| {
                if let Some(text) = v["text"].as_str() {
                    duration_ms =
                        duration_ms.or_else(|| text.split('•').find_map(|s| duration(s.trim())));
                    for part in text.split('•').map(str::trim) {
                        if let Some(count) = part
                            .strip_suffix(" plays")
                            .or_else(|| part.strip_suffix(" views"))
                        {
                            play_count = Some(count.to_owned());
                        }
                    }
                }
            });
        }
        results.push(SearchResult {
            id,
            title,
            artist,
            url,
            thumbnail: thumbnail(item),
            duration_ms,
            play_count,
            kind,
        });
    });
    results
}

fn parse_flat(bytes: &[u8]) -> Result<Vec<SearchResult>> {
    let value: Value = serde_json::from_slice(bytes)?;
    let entries = value["entries"]
        .as_array()
        .context("YouTube did not return any tracks")?;
    Ok(entries
        .iter()
        .filter_map(|v| {
            let id = v["id"].as_str()?;
            if id.len() != 11
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return None;
            }
            let title = v["track"].as_str().or(v["title"].as_str())?;
            if matches!(title, "[Deleted video]" | "[Private video]") {
                return None;
            }
            Some(SearchResult {
                id: id.into(),
                title: title.into(),
                artist: v["artist"]
                    .as_str()
                    .or(v["uploader"].as_str())
                    .or(v["channel"].as_str())
                    .unwrap_or_default()
                    .into(),
                url: format!("https://www.youtube.com/watch?v={id}"),
                thumbnail: thumbnail(v)
                    .or_else(|| Some(format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"))),
                duration_ms: v["duration"]
                    .as_f64()
                    .filter(|v| v.is_finite() && *v >= 0.)
                    .map(|v| (v * 1000.) as i64),
                play_count: v["view_count"].as_u64().map(|n| n.to_string()),
                kind: ResultKind::Track,
            })
        })
        .collect())
}

/// Reuse the downloader's source and normalized metadata matching rules.
pub fn collection_matches(local: &[Track], results: &[SearchResult]) -> HashMap<String, i64> {
    let entries: Vec<_> = results
        .iter()
        .map(|r| crate::download_duplicates::Entry {
            url: r.url.clone(),
            title: r.title.clone(),
            artist: r.artist.clone(),
            duration_ms: r.duration_ms,
        })
        .collect();
    crate::download_duplicates::inspect(local, &entries)
        .into_iter()
        .filter_map(|d| d.existing_id.map(|id| (results[d.index].id.clone(), id)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(kind: ResultKind, id: &str) -> SearchResult {
        SearchResult {
            id: id.into(),
            title: id.into(),
            artist: String::new(),
            url: format!("https://www.youtube.com/watch?v={id}"),
            thumbnail: None,
            duration_ms: None,
            play_count: None,
            kind,
        }
    }

    #[test]
    fn native_tracks_skip_slow_tool_fallback_and_merge_metadata() {
        let mut enriched = result(ResultKind::Track, "same");
        enriched.duration_ms = Some(123_000);
        let items = complete_search(
            vec![
                Ok(vec![result(ResultKind::Track, "same")]),
                Ok(vec![enriched, result(ResultKind::Album, "album")]),
            ],
            SearchFilters::default(),
            || panic!("Native results must not launch yt-dlp"),
        )
        .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].duration_ms, Some(123_000));
    }

    #[test]
    fn fallback_adds_missing_tracks_and_keeps_partial_music_results() {
        let items = complete_search(
            vec![
                Ok(vec![result(ResultKind::Album, "album")]),
                Err(anyhow::anyhow!("Playlist request failed")),
            ],
            SearchFilters::default(),
            || Ok(vec![result(ResultKind::Track, "video")]),
        )
        .unwrap();
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|r| r.kind == ResultKind::Track));
        let items = complete_search(
            vec![Ok(vec![result(ResultKind::Album, "album")])],
            SearchFilters::default(),
            || anyhow::bail!("Tools unavailable"),
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert!(
            complete_search(
                vec![Err(anyhow::anyhow!("Offline"))],
                SearchFilters::default(),
                || anyhow::bail!("Tools unavailable")
            )
            .is_err()
        );
    }

    #[test]
    fn collection_filters_never_start_video_fallback() {
        let filters = SearchFilters {
            tracks: false,
            albums: true,
            playlists: true,
        };
        let items = complete_search(
            vec![Ok(vec![
                result(ResultKind::Track, "exclude"),
                result(ResultKind::Playlist, "playlist"),
            ])],
            filters,
            || panic!("Track filter is disabled"),
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ResultKind::Playlist);
    }

    #[test]
    #[ignore = "Requires live YouTube network access"]
    fn live_discovery_latency() {
        for query in ["Daft", "Daft Punk", "Daft Punk get"] {
            let at = std::time::Instant::now();
            let items = suggestions(query).unwrap();
            assert!(!items.is_empty());
            eprintln!(
                "Suggestions {query:?}: {:.0} ms, {} items",
                at.elapsed().as_secs_f64() * 1000.,
                items.len()
            );
        }
        let at = std::time::Instant::now();
        let results = search_filtered(
            Path::new("missing-youtube-tools"),
            "Daft Punk",
            SearchFilters::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        for kind in [ResultKind::Track, ResultKind::Album, ResultKind::Playlist] {
            assert!(results.iter().any(|r| r.kind == kind), "Missing {kind:?}");
        }
        assert!(
            results
                .iter()
                .any(|r| r.kind == ResultKind::Track && r.duration_ms.is_some()),
            "Native search must retain track durations"
        );
        assert!(
            results
                .iter()
                .any(|r| r.kind == ResultKind::Track && r.play_count.is_some()),
            "Native search must retain available play counts"
        );
        eprintln!(
            "Mixed native search without tools: {:.0} ms, {} results",
            at.elapsed().as_secs_f64() * 1000.,
            results.len()
        );
    }
    #[test]
    fn empty_filters_do_not_resolve_tools_or_start_a_search() {
        let filters = SearchFilters {
            tracks: false,
            albums: false,
            playlists: false,
        };
        assert!(
            search_filtered(
                Path::new("missing-youtube-tools"),
                "",
                filters,
                &AtomicBool::new(false)
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            search_filtered(
                Path::new("missing-youtube-tools"),
                "query",
                filters,
                &AtomicBool::new(true)
            )
            .is_err()
        );
    }

    #[test]
    fn cached_queries_distinguish_all_filter_combinations() {
        let keys: HashSet<_> = (0..8)
            .map(|mask| {
                SearchFilters {
                    tracks: mask & 1 != 0,
                    albums: mask & 2 != 0,
                    playlists: mask & 4 != 0,
                }
                .cache_key("same query")
            })
            .collect();
        assert_eq!(keys.len(), 8);
    }

    #[test]
    #[ignore = "Requires YouTube Music network access; track search also needs yt-dlp"]
    fn live_filters_search_only_selected_types() {
        for filters in [
            SearchFilters {
                tracks: false,
                albums: true,
                playlists: false,
            },
            SearchFilters {
                tracks: false,
                albums: false,
                playlists: true,
            },
            SearchFilters {
                tracks: false,
                albums: true,
                playlists: true,
            },
            SearchFilters {
                tracks: true,
                albums: false,
                playlists: false,
            },
        ] {
            // Album/playlist searches must work without resolving local tools.
            let root = if filters.tracks {
                std::env::current_dir().unwrap()
            } else {
                PathBuf::from("missing-youtube-tools")
            };
            let results =
                search_filtered(&root, "Daft Punk", filters, &AtomicBool::new(false)).unwrap();
            assert!(!results.is_empty(), "Empty results for {filters:?}");
            assert!(
                results.iter().all(|r| filters.allows(r.kind)),
                "Unexpected category for {filters:?}"
            );
            for kind in [ResultKind::Track, ResultKind::Album, ResultKind::Playlist] {
                if filters.allows(kind) {
                    assert!(results.iter().any(|r| r.kind == kind), "Missing {kind:?}");
                }
            }
            eprintln!("Filtered search {filters:?}: {} results", results.len());
        }
    }

    #[test]
    fn album_uses_canonical_playlist_instead_of_a_menu_radio_mix() {
        let value = json!({"contents":{"watchEndpoint":{"playlistId":"RDAMVMradio"}},
            "microformat":{"microformatDataRenderer":{"urlCanonical":"https://music.youtube.com/playlist?list=OLAK5uyalbum"}}});
        assert_eq!(
            album_playlist_url(&value).unwrap(),
            "https://www.youtube.com/playlist?list=OLAK5uyalbum"
        );
        assert!(
            album_playlist_url(&json!({"watchPlaylistEndpoint":{"playlistId":"RDAMVMradio"}}))
                .is_err()
        );
    }
    #[test]
    #[ignore = "Requires YouTube network access and installed yt-dlp/FFmpeg"]
    fn live_discovery_collection_and_stream() {
        let query = "Daft Punk";
        assert!(!suggestions(query).expect("suggestions").is_empty());
        let root = std::env::current_dir().unwrap();
        let cancel = AtomicBool::new(false);
        let results = search(&root, query, &cancel).expect("search");
        assert!(
            results
                .iter()
                .any(|r| r.duration_ms.is_some() && r.play_count.is_some()),
            "Missing duration / play counts"
        );
        for kind in [ResultKind::Track, ResultKind::Album, ResultKind::Playlist] {
            assert!(
                results
                    .iter()
                    .any(|r| r.kind == kind && r.thumbnail.is_some()),
                "Missing {kind:?}"
            );
        }
        let album = results
            .iter()
            .find(|r| r.kind == ResultKind::Album)
            .unwrap();
        let tracks = collection(&root, album, &cancel).expect("album tracks");
        assert!(!tracks.is_empty());
        let playlist = results
            .iter()
            .find(|r| r.kind == ResultKind::Playlist)
            .unwrap();
        assert!(
            !collection(&root, playlist, &cancel)
                .expect("playlist tracks")
                .is_empty()
        );
        let url = stream_url(&root, &tracks[0], &cancel).expect("resolve stream");
        let mut response = agent(false)
            .get(&url)
            .header("Range", "bytes=0-4095")
            .call()
            .expect("open stream");
        let mut bytes = [0; 64];
        response
            .body_mut()
            .as_reader()
            .read_exact(&mut bytes)
            .expect("read stream bytes");
        eprintln!(
            "Live search: {} results, {} album tracks; stream bytes received",
            results.len(),
            tracks.len()
        );
    }
    #[test]
    fn suggestions_are_bounded_and_deduplicated() {
        assert_eq!(
            parse_suggestions(r#"["a",["a song","a song","another",""],{}]"#).unwrap(),
            vec!["a song", "another"]
        );
        assert!(parse_suggestions("{}").is_err());
    }
    #[test]
    fn music_types_and_artist_links() {
        let fixture = json!({"contents": [
            {"musicResponsiveListItemRenderer": {"flexColumns": [
                {"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"Song"}]}}},
                {"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"Artist","navigationEndpoint":{"browseEndpoint":{"browseId":"UCartist"}}},{"text":" • 3:20"}]}}}],
                "playlistItemData":{"videoId":"abcdefghijk"},"thumbnail":{"thumbnails":[{"url":"https://img/song.jpg"}]}}},
            {"musicTwoRowItemRenderer":{"title":{"runs":[{"text":"Album"}]},"navigationEndpoint":{"browseEndpoint":{"browseId":"MPREalbum"}}}},
            {"musicTwoRowItemRenderer":{"title":{"runs":[{"text":"Mix"}]},"navigationEndpoint":{"browseEndpoint":{"browseId":"VLPLmix"}}}},
            {"musicTwoRowItemRenderer":{"title":{"runs":[{"text":"Artist"}]},"navigationEndpoint":{"browseEndpoint":{"browseId":"UCartist"}}}}
        ]});
        let results = parse_music(&fixture);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].artist, "Artist");
        assert_eq!(results[0].duration_ms, Some(200000));
        assert_eq!(
            results[0].thumbnail.as_deref(),
            Some("https://img/song.jpg")
        );
        assert_eq!(results[1].kind, ResultKind::Album);
        assert_eq!(
            results[2].url,
            "https://www.youtube.com/playlist?list=PLmix"
        );
    }
    #[test]
    fn music_reads_fixed_duration_and_abbreviated_play_counts() {
        let value = json!({"musicResponsiveListItemRenderer": {
            "flexColumns": [
                {"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"Song"}]}}},
                {"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"Song • Artist"}]}}},
                {"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"1.2B plays"}]}}}
            ],
            "fixedColumns": [{"musicResponsiveListItemFixedColumnRenderer":{"text":{"runs":[{"text":"1:02:03"}]}}}],
            "playlistItemData":{"videoId":"abcdefghijk"}
        }});
        let results = parse_music(&value);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].duration_ms, Some(3_723_000));
        assert_eq!(results[0].play_count.as_deref(), Some("1.2B"));
        let results = parse_flat(br#"{"entries":[{"id":"abcdefghijk","title":"Video","duration":123.5,"view_count":12345678}]}"#).unwrap();
        assert_eq!(results[0].duration_ms, Some(123_500));
        assert_eq!(results[0].play_count.as_deref(), Some("12345678"));
    }
    #[test]
    fn flat_tracks_skip_unavailable_and_keep_artwork() {
        let results = parse_flat(br#"{"entries":[null,{"id":"abcdefghijk","title":"Song","duration":3.5},{"id":"private0000","title":"[Private video]"},{"id":"PLlist","title":"List"}]}"#).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].duration_ms, Some(3500));
        assert!(results[0].thumbnail.is_some());
    }
    #[test]
    fn collection_matches_video_and_normalized_metadata() {
        let local = vec![Track {
            id: 7,
            title: " Song ".into(),
            artist: "ARTIST".into(),
            source_url: Some("https://youtu.be/abcdefghijk".into()),
            file_path: "audio/a.mp3".into(),
            duration_ms: Some(200000),
            created_at: String::new(),
        }];
        let make = |id: &str, title: &str, artist: &str| SearchResult {
            id: id.into(),
            title: title.into(),
            artist: artist.into(),
            url: format!("https://www.youtube.com/watch?v={id}"),
            thumbnail: None,
            duration_ms: Some(200000),
            play_count: None,
            kind: ResultKind::Track,
        };
        let found = collection_matches(
            &local,
            &[
                make("abcdefghijk", "Different", ""),
                make("other000000", "song", "artist"),
                make("live0000000", "song live", "artist"),
            ],
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found["abcdefghijk"], 7);
    }
}
