use crate::{download_tools::DownloadTools, models::Track, services::validate_youtube_url};
use crate::{i18n::tr, localized_format};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct YoutubeProgress {
    pub completed: usize,
    pub total: usize,
    pub current_title: Option<String>,
}

impl YoutubeProgress {
    pub fn message(&self) -> String {
        if self.total == 0 {
            return tr("Получаем список треков…").into();
        }
        match &self.current_title {
            Some(title) => localized_format!(
                "Скачиваем {} из {}: {}",
                "Downloading {} of {}: {}",
                self.completed + 1,
                self.total,
                title
            ),
            None => localized_format!(
                "Обработано треков: {} из {}",
                "Tracks processed: {} of {}",
                self.completed,
                self.total
            ),
        }
    }
}

#[derive(Debug)]
pub struct YoutubeFailure {
    pub title: String,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct YoutubeDownloadReport {
    pub tracks: Vec<Track>,
    pub failures: Vec<YoutubeFailure>,
    pub total: usize,
    pub playlist_id: Option<i64>,
    pub cancelled: bool,
}

impl YoutubeDownloadReport {
    pub fn message(&self) -> String {
        localized_format!(
            "{}Добавлено треков: {} из {}. Ошибок: {}.",
            "{}Tracks added: {} of {}. Errors: {}.",
            if self.cancelled {
                tr("Загрузка остановлена. ")
            } else {
                ""
            },
            self.tracks.len(),
            self.total,
            self.failures.len()
        )
    }
    pub fn error_details(&self) -> Option<String> {
        (!self.failures.is_empty()).then(|| {
            self.failures
                .iter()
                .map(|error| format!("{}: {}", error.title, error.message))
                .collect::<Vec<_>>()
                .join("\n\n")
        })
    }
}

#[derive(Default, Deserialize)]
pub(crate) struct DownloadInfo {
    pub title: Option<String>,
    pub track: Option<String>,
    pub artist: Option<String>,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
}

pub(crate) struct YoutubeEntry {
    pub title: String,
    pub url: Option<url::Url>,
}
pub(crate) struct YoutubePlaylist {
    pub title: String,
    pub entries: Vec<YoutubeEntry>,
}
pub(crate) struct DownloadedAudio {
    pub path: PathBuf,
    pub info: DownloadInfo,
}

pub(crate) trait YoutubeSource {
    fn playlist(&self, url: &url::Url, cancel: &AtomicBool) -> Result<YoutubePlaylist>;
    fn track(&self, url: &url::Url, staging: &Path, cancel: &AtomicBool)
    -> Result<DownloadedAudio>;
}

pub(crate) struct YoutubeClient {
    tools: DownloadTools,
}

impl YoutubeClient {
    pub(crate) fn search_candidates(
        &self,
        title: &str,
        artist: &str,
        duration_ms: Option<u64>,
        cancel: &AtomicBool,
    ) -> Result<Vec<url::Url>> {
        let mut command = self.command();
        command
            .args([
                "--flat-playlist",
                "--dump-single-json",
                "--skip-download",
                "--",
            ])
            .arg(format!("ytsearch5:{artist} {title} audio"));
        let output = run_command(command, cancel, Duration::from_secs(120))?;
        ensure!(output.success, "yt-dlp: {}", output.error());
        select_search_results(&output.stdout, title, artist, duration_ms)
    }

    pub fn new(data_directory: &Path) -> Result<Self> {
        Ok(Self {
            tools: DownloadTools::discover(data_directory)?,
        })
    }
    fn command(&self) -> Command {
        let mut command = self.tools.command();
        command.args([
            "--ignore-config",
            "--no-colors",
            "--no-progress",
            "--socket-timeout",
            "30",
            "--retries",
            "2",
        ]);
        command
    }
}

fn normalized(input: &str) -> Vec<String> {
    input
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn select_search_results(
    json: &[u8],
    title: &str,
    artist: &str,
    duration_ms: Option<u64>,
) -> Result<Vec<url::Url>> {
    let data: serde_json::Value = serde_json::from_slice(json)?;
    let entries = data["entries"]
        .as_array()
        .context(tr("YouTube не вернул результаты поиска"))?;
    let title_words = normalized(title);
    let artist_words = normalized(artist);
    let mut candidates = Vec::new();
    for entry in entries {
        let name = entry["title"].as_str().unwrap_or_default();
        let words = normalized(&format!(
            "{name} {} {}",
            entry["uploader"].as_str().unwrap_or_default(),
            entry["channel"].as_str().unwrap_or_default()
        ));
        let coverage = |needle: &[String]| {
            needle.iter().filter(|w| words.contains(w)).count() as f64 / needle.len().max(1) as f64
        };
        let title_score = coverage(&title_words);
        let artist_score = coverage(&artist_words);
        if title_score < 0.6 || artist_score < 0.3 {
            continue;
        }
        let mut score = title_score * 6. + artist_score * 4.;
        if let (Some(expected), Some(actual)) = (duration_ms, entry["duration"].as_f64()) {
            let difference = (expected as f64 / 1000. - actual).abs();
            if difference > 30_f64.max(expected as f64 / 1000. * 0.2) {
                continue;
            }
            score += 3. * (1. - difference / 30.).max(0.);
        }
        let requested = normalized(&format!("{title} {artist}"));
        for variant in ["cover", "remix", "live", "karaoke", "sped", "slowed"] {
            if words.iter().any(|w| w == variant) && !requested.iter().any(|w| w == variant) {
                score -= 4.;
            }
        }
        if let Some(id) = entry["id"].as_str().filter(|id| {
            id.len() == 11
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        }) {
            candidates.push((score, format!("https://www.youtube.com/watch?v={id}")));
        }
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut urls = Vec::new();
    for (_, candidate) in candidates.into_iter().filter(|c| c.0 >= 7.) {
        let url = url::Url::parse(&candidate)?;
        if !urls.contains(&url) {
            urls.push(url);
        }
    }
    ensure!(
        !urls.is_empty(),
        tr("На YouTube не найдено подходящего совпадения")
    );
    Ok(urls)
}

impl YoutubeSource for YoutubeClient {
    fn playlist(&self, url: &url::Url, cancel: &AtomicBool) -> Result<YoutubePlaylist> {
        let mut command = self.command();
        command
            .args([
                "--yes-playlist",
                "--flat-playlist",
                "--dump-single-json",
                "--skip-download",
                "--ignore-errors",
                "--",
            ])
            .arg(url.as_str());
        let output = run_command(command, cancel, Duration::from_secs(180))?;
        match parse_playlist(&output.stdout) {
            Ok(playlist) => Ok(playlist),
            Err(_) if !output.success => bail!("yt-dlp: {}", output.error()),
            Err(error) => Err(error),
        }
    }

    fn track(
        &self,
        url: &url::Url,
        staging: &Path,
        cancel: &AtomicBool,
    ) -> Result<DownloadedAudio> {
        let mut command = self.command();
        command
            .args([
                "--no-playlist",
                "--no-overwrites",
                "--write-info-json",
                "--extract-audio",
                "--audio-format",
                "mp3",
                "--audio-quality",
                "0",
                "--output",
            ])
            .arg(staging.join("track.%(ext)s"))
            .arg("--")
            .arg(url.as_str());
        let output = run_command(command, cancel, Duration::from_secs(600))?;
        ensure!(output.success, "yt-dlp: {}", output.error());
        let info = serde_json::from_slice(&fs::read(staging.join("track.info.json"))?)
            .context(tr("Не удалось прочитать метаданные загрузки"))?;
        Ok(DownloadedAudio {
            path: staging.join("track.mp3"),
            info,
        })
    }
}

/// A video URL carrying `list` means the entire playlist, including when it is a youtu.be link.
pub(crate) fn playlist_url(url: &url::Url) -> Option<url::Url> {
    let list = url.query_pairs().find_map(|(key, value)| {
        (key == "list" && !value.trim().is_empty()).then(|| value.into_owned())
    })?;
    let mut playlist = url::Url::parse("https://www.youtube.com/playlist").expect("static URL");
    playlist.query_pairs_mut().append_pair("list", &list);
    Some(playlist)
}

#[derive(Deserialize)]
struct FlatPlaylist {
    title: Option<String>,
    entries: Vec<Option<FlatEntry>>,
}
#[derive(Deserialize)]
struct FlatEntry {
    id: Option<String>,
    title: Option<String>,
    url: Option<String>,
    webpage_url: Option<String>,
}

fn parse_playlist(json: &[u8]) -> Result<YoutubePlaylist> {
    let flat: FlatPlaylist =
        serde_json::from_slice(json).context(tr("Не удалось прочитать список треков YouTube"))?;
    ensure!(
        !flat.entries.is_empty(),
        tr("В плейлисте нет доступных для просмотра треков")
    );
    let entries = flat
        .entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let Some(entry) = entry else {
                return YoutubeEntry {
                    title: localized_format!(
                        "Недоступный трек №{}",
                        "Unavailable track #{}",
                        index + 1
                    ),
                    url: None,
                };
            };
            let url = entry
                .id
                .filter(|id| {
                    !id.is_empty()
                        && id.len() <= 64
                        && id
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                })
                .and_then(|id| {
                    url::Url::parse(&format!("https://www.youtube.com/watch?v={id}")).ok()
                })
                .or_else(|| {
                    entry
                        .webpage_url
                        .or(entry.url)
                        .and_then(|url| validate_youtube_url(&url).ok())
                        .filter(|url| playlist_url(url).is_none())
                });
            YoutubeEntry {
                title: entry
                    .title
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| localized_format!("Трек №{}", "Track #{}", index + 1)),
                url,
            }
        })
        .collect();
    Ok(YoutubePlaylist {
        title: flat
            .title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or(tr("Плейлист YouTube").into()),
        entries,
    })
}

#[derive(Debug)]
struct ProcessOutput {
    success: bool,
    stdout: Vec<u8>,
    stderr: String,
}
impl ProcessOutput {
    fn error(&self) -> &str {
        if self.stderr.trim().is_empty() {
            tr("Загрузчик завершился с ошибкой")
        } else {
            self.stderr.trim()
        }
    }
}

fn run_command(
    mut command: Command,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<ProcessOutput> {
    ensure!(!cancel.load(Ordering::Relaxed), tr("Загрузка остановлена"));
    // File-backed output avoids pipe deadlocks for large playlists and downloader logs.
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    command
        .stdout(Stdio::from(stdout.try_clone()?))
        .stderr(Stdio::from(stderr.try_clone()?));
    let mut process =
        crate::process_tree::DownloadProcess::spawn(&mut command).with_context(|| {
            localized_format!(
                "Не удалось запустить загрузчик {}",
                "Could not start downloader {}",
                command.get_program().to_string_lossy()
            )
        })?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
            process.stop();
            if cancel.load(Ordering::Relaxed) {
                bail!(tr("Загрузка остановлена"));
            }
            bail!(
                "{}",
                localized_format!(
                    "Превышено время ожидания YouTube ({} мин)",
                    "YouTube timed out ({} min)",
                    timeout.as_secs() / 60
                )
            );
        }
        if let Some(status) = process.child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(150));
    };
    // Release the whole tree even after success: helpers must not remain idle in the background.
    drop(process);
    stdout.seek(SeekFrom::Start(0))?;
    stderr.seek(SeekFrom::Start(0))?;
    let mut output = Vec::new();
    stdout.take(128 * 1024 * 1024).read_to_end(&mut output)?;
    let mut errors = Vec::new();
    stderr.take(32_768).read_to_end(&mut errors)?;
    Ok(ProcessOutput {
        success: status.success(),
        stdout: output,
        stderr: String::from_utf8_lossy(&errors).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_prefers_original_and_checks_artist_and_duration() {
        let data = br#"{"entries":[{"id":"aaaaaaaaaaa","title":"Artist Song live","duration":212},{"id":"bbbbbbbbbbb","title":"Artist Song official audio","duration":210},{"id":"ccccccccccc","title":"Someone Song","duration":210},{"id":"ddddddddddd","title":"Artist Song full album","duration":3600}]}"#;
        let candidates = select_search_results(data, "Song", "Artist", Some(210000)).unwrap();
        assert_eq!(candidates.len(), 2);
        assert!(candidates[0].as_str().ends_with("bbbbbbbbbbb"));
        assert!(candidates[1].as_str().ends_with("aaaaaaaaaaa"));
        assert!(select_search_results(data, "Different", "Other", None).is_err());
    }
    #[test]
    fn recognizes_playlist_links_and_strips_video_context() {
        for input in [
            "https://www.youtube.com/playlist?list=PL123",
            "https://youtube.com/watch?v=video&list=PL123&index=4",
            "https://youtu.be/video?list=PL123",
            "https://music.youtube.com/playlist?list=PL123",
        ] {
            assert_eq!(
                playlist_url(&validate_youtube_url(input).unwrap())
                    .unwrap()
                    .as_str(),
                "https://www.youtube.com/playlist?list=PL123"
            );
        }
        assert!(playlist_url(&validate_youtube_url("https://youtu.be/video").unwrap()).is_none());
        assert!(
            playlist_url(&validate_youtube_url("https://youtube.com/watch?v=video&list=").unwrap())
                .is_none()
        );
    }
    #[test]
    fn playlist_parser_preserves_order_duplicates_and_unavailable_entries() {
        let playlist = parse_playlist(br#"{"title":"Collection","entries":[{"id":"first","title":"One"},null,{"id":"second","title":"Two"},{"id":"first","title":"Again"},{"url":"https://evil.test/payload"}]}"#).unwrap();
        assert_eq!(playlist.title, "Collection");
        assert_eq!(playlist.entries.len(), 5);
        assert!(playlist.entries[1].url.is_none());
        assert_eq!(
            playlist.entries[2].url.as_ref().unwrap().query(),
            Some("v=second")
        );
        assert_eq!(playlist.entries[0].url, playlist.entries[3].url);
        assert!(playlist.entries[4].url.is_none());
        assert!(parse_playlist(br#"{"entries":[]}"#).is_err());
        assert!(parse_playlist(b"invalid").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn cancelling_interrupts_a_running_downloader_process() {
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let trigger = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            signal.store(true, Ordering::Relaxed);
        });
        let mut command = Command::new(
            PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/ping.exe"),
        );
        command.args(["-n", "30", "127.0.0.1"]);
        let started = Instant::now();
        let error = run_command(command, &cancel, Duration::from_secs(60)).unwrap_err();
        trigger.join().unwrap();
        assert!(error.to_string().contains("остановлена"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
