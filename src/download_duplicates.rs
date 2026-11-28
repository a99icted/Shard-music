//! Inspect internet collections before downloading audio using LAN sync's metadata rules.
use crate::{
    models::Track,
    sync::{durations_match, metadata_key},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize)]
pub(crate) struct Entry {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Duplicate {
    pub index: usize,
    pub existing_id: Option<i64>,
    pub earlier_index: Option<usize>,
    pub existing_title: String,
    pub kind: &'static str,
}

fn source_key(source: &str) -> String {
    if let Ok(url) = url::Url::parse(source) {
        let video = match url.host_str() {
            Some("youtu.be") => url
                .path_segments()
                .and_then(|mut s| s.next())
                .map(str::to_owned),
            Some("youtube.com" | "www.youtube.com" | "music.youtube.com" | "m.youtube.com") => url
                .query_pairs()
                .find(|(k, _)| k == "v")
                .map(|(_, v)| v.into_owned())
                .or_else(|| url.path().strip_prefix("/shorts/").map(str::to_owned)),
            _ => None,
        };
        if let Some(id) = video.filter(|s| !s.is_empty()) {
            return format!("youtube:{id}");
        }
    }
    source.to_owned()
}

pub(crate) fn inspect(local: &[Track], entries: &[Entry]) -> Vec<Duplicate> {
    let mut sources = HashMap::new();
    let mut metadata = HashMap::<String, Vec<&Track>>::new();
    for track in local {
        if let Some(source) = track.source_url.as_ref().filter(|s| !s.is_empty()) {
            sources.entry(source_key(source)).or_insert(track);
        }
        if let Some(key) = metadata_key(&track.title, &track.artist) {
            metadata.entry(key).or_default().push(track);
        }
    }
    let mut earlier_sources = HashMap::new();
    let mut earlier_metadata = HashMap::<String, Vec<usize>>::new();
    let mut duplicates = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let source = source_key(&entry.url);
        let key = metadata_key(&entry.title, &entry.artist);
        let candidate = sources
            .get(&source)
            .map(|t| (*t, "same_source"))
            .or_else(|| {
                key.as_ref()
                    .and_then(|k| metadata.get(k))
                    .and_then(|items| {
                        items
                            .iter()
                            .find(|t| durations_match(t.duration_ms, entry.duration_ms))
                    })
                    .map(|t| (*t, "similar_metadata"))
            });
        if let Some((track, kind)) = candidate {
            duplicates.push(Duplicate {
                index,
                existing_id: Some(track.id),
                earlier_index: None,
                existing_title: track.title.clone(),
                kind,
            });
        } else {
            let earlier = earlier_sources
                .get(&source)
                .copied()
                .map(|i| (i, "same_source"))
                .or_else(|| {
                    key.as_ref()
                        .and_then(|k| earlier_metadata.get(k))
                        .and_then(|ids| {
                            ids.iter().find(|i| {
                                durations_match(entries[**i].duration_ms, entry.duration_ms)
                            })
                        })
                        .map(|i| (*i, "similar_metadata"))
                });
            if let Some((i, kind)) = earlier {
                duplicates.push(Duplicate {
                    index,
                    existing_id: None,
                    earlier_index: Some(i),
                    existing_title: entries[i].title.clone(),
                    kind,
                });
            }
        }
        if !source.is_empty() {
            earlier_sources.entry(source).or_insert(index);
        }
        if let Some(key) = key {
            earlier_metadata.entry(key).or_default().push(index);
        }
    }
    duplicates
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(url: &str, title: &str, artist: &str, duration: Option<i64>) -> Entry {
        Entry {
            url: url.into(),
            title: title.into(),
            artist: artist.into(),
            duration_ms: duration,
        }
    }
    #[test]
    fn canonical_sources_and_repeated_entries_are_detected() {
        let entries = vec![
            entry("https://youtu.be/abc?t=10", "Song", "Artist", None),
            entry(
                "https://music.youtube.com/watch?v=abc&list=xyz",
                "Other title",
                "",
                None,
            ),
            entry(
                "https://www.youtube.com/watch?v=def",
                "Song",
                "Artist",
                None,
            ),
        ];
        let found = inspect(&[], &entries);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].index, 1);
        assert_eq!(found[0].earlier_index, Some(0));
    }
    #[test]
    fn local_metadata_requires_artist_and_nearby_duration() {
        let track = Track {
            id: 42,
            title: " Song ".into(),
            artist: "ARTIST".into(),
            source_url: Some("https://www.youtube.com/watch?v=original&list=xyz".into()),
            duration_ms: Some(180000),
            file_path: "audio/a.m4a".into(),
            created_at: String::new(),
        };
        let entries = vec![
            entry("https://youtu.be/original", "", "", None),
            entry("https://youtu.be/other", "song", "artist", Some(182000)),
            entry("https://youtu.be/live", "song", "artist", Some(240000)),
            entry("https://youtu.be/unknown", "song", "", Some(180000)),
        ];
        let found = inspect(&[track], &entries);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].existing_id, Some(42));
        assert_eq!(found[1].kind, "similar_metadata");
    }

    #[test]
    fn missing_sources_are_not_matches_by_themselves() {
        assert!(
            inspect(
                &[],
                &[
                    entry("", "Unavailable one", "", None),
                    entry("", "Unavailable two", "", None)
                ]
            )
            .is_empty()
        );
    }
}
