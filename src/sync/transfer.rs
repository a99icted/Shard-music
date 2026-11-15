use super::*;
use rusqlite::{OptionalExtension, params};
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::{Seek, SeekFrom},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncSelection {
    Playlist(i64),
    Track(i64),
    Library,
}

#[derive(Clone, Copy, Debug)]
pub struct SyncOptions {
    pub warn_duplicates: bool,
    /// Reuse an existing library entry instead of importing another copy.
    pub ignore_duplicates: bool,
}
impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            warn_duplicates: true,
            ignore_duplicates: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuplicateKind {
    IdenticalFile,
    SameSource,
    SimilarMetadata,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DuplicateTrack {
    pub index: usize,
    pub title: String,
    pub artist: String,
    pub existing_title: String,
    pub kind: DuplicateKind,
}
#[derive(Clone, Copy, Debug)]
pub struct DuplicateDecision {
    pub index: usize,
    pub copy: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TransferReport {
    pub copied: usize,
    pub reused: usize,
    pub resumed_bytes: u64,
}

#[derive(Clone, Copy)]
enum Match {
    Local(i64),
    Earlier(usize),
    New,
}
struct Plan {
    matches: Vec<Match>,
    duplicates: Vec<DuplicateTrack>,
}

impl SyncService {
    // Quiet helpers retain the original API for callers that only want additive sync.
    pub fn send_playlist(&self, id: i64) -> Result<()> {
        self.send_selection(
            SyncSelection::Playlist(id),
            SyncOptions {
                warn_duplicates: false,
                ignore_duplicates: true,
            },
            |_| unreachable!(),
        )
        .map(|_| ())
    }
    pub fn send_track(&self, id: i64) -> Result<()> {
        self.send_selection(
            SyncSelection::Track(id),
            SyncOptions {
                warn_duplicates: false,
                ignore_duplicates: true,
            },
            |_| unreachable!(),
        )
        .map(|_| ())
    }
    pub fn send_selection(
        &self,
        selection: SyncSelection,
        options: SyncOptions,
        mut decide: impl FnMut(&[DuplicateTrack]) -> Result<Vec<DuplicateDecision>>,
    ) -> Result<TransferReport> {
        let _guard = self.guard()?;
        let result = self.send_selection_inner(selection, options, &mut decide);
        if let Err(error) = &result {
            self.inner.status.lock().unwrap().error = Some(format!("{error:#}"));
        }
        result
    }
    fn send_selection_inner(
        &self,
        selection: SyncSelection,
        options: SyncOptions,
        decide: &mut impl FnMut(&[DuplicateTrack]) -> Result<Vec<DuplicateDecision>>,
    ) -> Result<TransferReport> {
        let db = &self.inner.db;
        let lists = PlaylistService::new(db.clone());
        let (title, playlist, favorites, library, tracks, scope) = match selection {
            SyncSelection::Playlist(id) => {
                let title = if id == FAVORITES_ID {
                    "Избранное".into()
                } else {
                    lists
                        .list_playlists()?
                        .into_iter()
                        .find(|p| p.id == id)
                        .context("Плейлист не найден")?
                        .name
                };
                (
                    title,
                    Some(id.to_string()),
                    id == FAVORITES_ID,
                    false,
                    lists.list_playlist_tracks(id)?,
                    format!("playlist:{id}"),
                )
            }
            SyncSelection::Track(id) => {
                let track = TrackRepository::new(db.clone()).get(id)?;
                (
                    track.title.clone(),
                    None,
                    false,
                    false,
                    vec![track],
                    format!("track:{id}"),
                )
            }
            SyncSelection::Library => (
                "Все треки".into(),
                None,
                false,
                true,
                TrackRepository::new(db.clone()).list_tracks()?,
                "library".into(),
            ),
        };
        self.progress(&title, false, 0, 0, false);
        let peer = self
            .inner
            .peer
            .lock()
            .unwrap()
            .clone()
            .context("Подключите устройство в настройках синхронизации")?;
        let target = PairingCode::parse(&peer.code)?;
        let audio = TrackService::new(db.clone());
        let favorites_ids = FavoritesService::new(db.clone()).ids()?;
        let mut files = Vec::new();
        let mut entries = Vec::new();
        let conn = db.connect()?;
        for track in tracks {
            self.active()?;
            let path = audio.audio_path(&track)?;
            let size = fs::metadata(&path)?.len();
            ensure!(
                size > 0 && size <= MAX_FILE,
                "Аудиофайл слишком большой или пустой"
            );
            let hash = indexed_hash(&conn, &track, &path, || self.active())?;
            entries.push(WireTrack {
                source_id: track.id.to_string(),
                title: track.title,
                artist: track.artist,
                source_url: track.source_url,
                duration_ms: track.duration_ms,
                extension: path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .into(),
                hash,
                size,
                favorite: favorites_ids.contains(&track.id),
            });
            files.push(path);
        }
        let mut manifest = Manifest {
            transfer_id: Uuid::new_v4().to_string(),
            version: VERSION,
            device: self.inner.device.clone(),
            playlist,
            title: title.clone(),
            favorites,
            library,
            tracks: entries,
        };
        let fingerprint = fingerprint(&manifest)?;
        // One durable operation ID survives retries, including a lost final acknowledgement.
        let scope = format!("{}:{scope}", peer.device);
        let previous = conn
            .query_row(
                "SELECT transfer_id FROM sync_outgoing WHERE scope=?1 AND fingerprint=?2",
                params![scope, fingerprint],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        if let Some(id) = previous {
            manifest.transfer_id = id;
        } else {
            conn.execute("INSERT INTO sync_outgoing(scope,fingerprint,transfer_id) VALUES (?1,?2,?3) ON CONFLICT(scope) DO UPDATE SET fingerprint=excluded.fingerprint,transfer_id=excluded.transfer_id", params![scope,fingerprint,manifest.transfer_id])?;
        }
        validate_manifest(&manifest)?;
        let inspection = {
            let mut stream = connect(&target)?;
            stream.set_read_timeout(Some(Duration::from_secs(600)))?;
            write_json(
                &mut stream,
                &Request::Inspect {
                    token: target.token.clone(),
                    manifest: manifest.clone(),
                },
            )?;
            read_json::<Reply>(&mut stream)?.check()?
        };
        ensure!(
            inspection.duplicates.len() <= manifest.tracks.len(),
            "Некорректный список дубликатов"
        );
        let mut duplicate_indexes = HashSet::new();
        for duplicate in &inspection.duplicates {
            let entry = manifest
                .tracks
                .get(duplicate.index)
                .context("Некорректный список дубликатов")?;
            ensure!(
                duplicate_indexes.insert(duplicate.index)
                    && duplicate.title == entry.title
                    && duplicate.artist == entry.artist
                    && duplicate.existing_title.chars().count() <= 500,
                "Некорректный список дубликатов"
            );
        }
        let copies = if inspection.done {
            vec![]
        } else if options.warn_duplicates && !inspection.duplicates.is_empty() {
            // No socket or SQLite transaction is held while the user decides.
            let decisions = decide(&inspection.duplicates)?;
            let decided: HashSet<_> = decisions.iter().map(|d| d.index).collect();
            ensure!(
                decided == duplicate_indexes && decisions.len() == decided.len(),
                "Нужно выбрать действие для каждого дубликата"
            );
            decisions
                .into_iter()
                .filter(|d| d.copy)
                .map(|d| d.index)
                .collect()
        } else if options.ignore_duplicates {
            vec![]
        } else {
            inspection.duplicates.iter().map(|d| d.index).collect()
        };
        self.active()?;
        let mut stream = connect(&target)?;
        stream.set_read_timeout(Some(Duration::from_secs(600)))?;
        write_json(
            &mut stream,
            &Request::Transfer {
                token: target.token,
                manifest: manifest.clone(),
                copies,
            },
        )?;
        let reply = read_json::<Reply>(&mut stream)?.check()?;
        ensure!(
            reply.missing.len() == reply.offsets.len(),
            "Некорректный список файлов от устройства"
        );
        let mut seen = HashSet::new();
        let mut total = 0u64;
        let mut sent = 0u64;
        for (&index, &offset) in reply.missing.iter().zip(&reply.offsets) {
            let entry = manifest
                .tracks
                .get(index)
                .context("Некорректный список файлов от устройства")?;
            ensure!(
                seen.insert(index) && offset <= entry.size,
                "Некорректный список файлов от устройства"
            );
            total += entry.size;
            sent += offset;
        }
        self.progress(&title, false, sent, total, false);
        for (index, offset) in reply.missing.into_iter().zip(reply.offsets) {
            let mut file = File::open(&files[index])?;
            file.seek(SeekFrom::Start(offset))?;
            let mut remaining = manifest.tracks[index].size - offset;
            let mut buffer = [0; 64 * 1024];
            while remaining > 0 {
                self.active()?;
                let limit = (remaining as usize).min(buffer.len());
                let size = file.read(&mut buffer[..limit])?;
                ensure!(size > 0, "Аудиофайл изменился во время передачи");
                stream.write_all(&buffer[..size])?;
                remaining -= size as u64;
                sent += size as u64;
                self.progress(&title, false, sent, total, false);
            }
        }
        let report = read_json::<Reply>(&mut stream)?.check()?.report;
        conn.execute(
            "DELETE FROM sync_outgoing WHERE scope=?1 AND transfer_id=?2",
            params![scope, manifest.transfer_id],
        )?;
        self.progress(&title, false, total, total, true);
        Ok(report)
    }
    fn completed(&self, manifest: &Manifest) -> Result<bool> {
        let previous = self
            .inner
            .db
            .connect()?
            .query_row(
                "SELECT fingerprint FROM sync_receipts WHERE source_device=?1 AND transfer_id=?2",
                params![manifest.device, manifest.transfer_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        if let Some(previous) = previous {
            ensure!(
                previous == fingerprint(manifest)?,
                "ID передачи уже использован для другой коллекции"
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub(super) fn inspect(&self, stream: &mut TcpStream, manifest: &Manifest) -> Result<()> {
        validate_manifest(manifest)?;
        let done = self.completed(manifest)?;
        let duplicates = if done {
            vec![]
        } else {
            self.plan(manifest, &[])?.duplicates
        };
        write_json(
            stream,
            &Reply {
                done,
                duplicates,
                ..Reply::ok()
            },
        )
    }
    fn plan(&self, manifest: &Manifest, copies: &[usize]) -> Result<Plan> {
        let db = &self.inner.db;
        let conn = db.connect()?;
        let audio = TrackService::new(db.clone());
        let mut local = Vec::new();
        let mut hashes = HashMap::<String, Vec<usize>>::new();
        let mut metadata = HashMap::<String, Vec<usize>>::new();
        let mut sources = HashMap::<String, Vec<usize>>::new();
        let mut local_ids = HashSet::new();
        for track in TrackRepository::new(db.clone()).list_tracks()? {
            self.active()?;
            let path = audio.audio_path(&track)?;
            if !path.is_file() {
                continue;
            }
            let hash = indexed_hash(&conn, &track, &path, || self.active())?;
            let index = local.len();
            local_ids.insert(track.id);
            hashes.entry(hash.clone()).or_default().push(index);
            if let Some(key) = metadata_key(&track.title, &track.artist) {
                metadata.entry(key).or_default().push(index);
            }
            if let Some(source) = track.source_url.as_ref().filter(|s| !s.is_empty()) {
                sources.entry(source.clone()).or_default().push(index);
            }
            local.push((track, hash));
        }
        let forced: HashSet<_> = copies.iter().copied().collect();
        ensure!(
            forced.len() == copies.len() && copies.iter().all(|i| *i < manifest.tracks.len()),
            "Некорректные решения по дубликатам"
        );
        let mut matches = Vec::new();
        let mut duplicates = Vec::new();
        let mut earlier = HashMap::<String, usize>::new();
        let mut earlier_sources = HashMap::<String, usize>::new();
        let mut earlier_metadata = HashMap::<String, Vec<usize>>::new();
        for (index, entry) in manifest.tracks.iter().enumerate() {
            let previous = earlier
                .get(&entry.hash)
                .copied()
                .map(|i| (i, DuplicateKind::IdenticalFile))
                .or_else(|| {
                    entry
                        .source_url
                        .as_ref()
                        .and_then(|source| earlier_sources.get(source))
                        .copied()
                        .map(|i| (i, DuplicateKind::SameSource))
                })
                .or_else(|| {
                    metadata_key(&entry.title, &entry.artist)
                        .and_then(|key| earlier_metadata.get(&key))
                        .and_then(|ids| {
                            ids.iter().find(|i| {
                                durations_match(manifest.tracks[**i].duration_ms, entry.duration_ms)
                            })
                        })
                        .map(|i| (*i, DuplicateKind::SimilarMetadata))
                });
            earlier.entry(entry.hash.clone()).or_insert(index);
            if let Some(source) = entry.source_url.as_ref().filter(|s| !s.is_empty()) {
                earlier_sources.entry(source.clone()).or_insert(index);
            }
            if let Some(key) = metadata_key(&entry.title, &entry.artist) {
                earlier_metadata.entry(key).or_default().push(index);
            }
            // Previously synchronized source tracks are incremental updates, not new duplicates.
            let origin = if manifest.playlist.is_some() || manifest.library {
                conn.query_row("SELECT track_id FROM sync_origins WHERE source_device=?1 AND source_track=?2 AND content_hash=?3", params![manifest.device,entry.source_id,entry.hash], |r| r.get::<_, i64>(0)).optional()?
                    .filter(|id| local_ids.contains(id))
            } else {
                None
            };
            if let Some(id) = origin {
                matches.push(Match::Local(id));
                continue;
            }
            let candidate = hashes
                .get(&entry.hash)
                .and_then(|ids| ids.first())
                .map(|i| (*i, DuplicateKind::IdenticalFile))
                .or_else(|| {
                    entry
                        .source_url
                        .as_ref()
                        .and_then(|source| sources.get(source))
                        .and_then(|ids| ids.first())
                        .map(|i| (*i, DuplicateKind::SameSource))
                })
                .or_else(|| {
                    metadata_key(&entry.title, &entry.artist)
                        .and_then(|key| metadata.get(&key))
                        .and_then(|ids| {
                            ids.iter().find(|i| {
                                durations_match(local[**i].0.duration_ms, entry.duration_ms)
                            })
                        })
                        .map(|i| (*i, DuplicateKind::SimilarMetadata))
                });
            let (matched, duplicate) = if let Some((i, kind)) = candidate {
                (
                    Match::Local(local[i].0.id),
                    Some(DuplicateTrack {
                        index,
                        title: entry.title.clone(),
                        artist: entry.artist.clone(),
                        existing_title: local[i].0.title.clone(),
                        kind,
                    }),
                )
            } else if let Some((previous, kind)) = previous {
                (
                    Match::Earlier(previous),
                    Some(DuplicateTrack {
                        index,
                        title: entry.title.clone(),
                        artist: entry.artist.clone(),
                        existing_title: manifest.tracks[previous].title.clone(),
                        kind,
                    }),
                )
            } else {
                (Match::New, None)
            };
            if let Some(duplicate) = duplicate {
                duplicates.push(duplicate);
            }
            matches.push(if forced.contains(&index) {
                Match::New
            } else {
                matched
            });
        }
        Ok(Plan {
            matches,
            duplicates,
        })
    }
    pub(super) fn receive(
        &self,
        stream: &mut TcpStream,
        manifest: Manifest,
        copies: &[usize],
    ) -> Result<()> {
        validate_manifest(&manifest)?;
        if self.completed(&manifest)? {
            write_json(
                stream,
                &Reply {
                    done: true,
                    ..Reply::ok()
                },
            )?;
            self.progress(&manifest.title, true, 0, 0, true);
            return write_json(
                stream,
                &Reply {
                    report: TransferReport {
                        reused: manifest.tracks.len(),
                        ..Default::default()
                    },
                    ..Reply::ok()
                },
            );
        }
        let db = &self.inner.db;
        let plan = self.plan(&manifest, copies)?;
        fs::create_dir_all(db.root().join("sync-cache"))?;
        let mut missing = Vec::new();
        let mut offsets = Vec::new();
        let mut requested = HashSet::new();
        for (index, matched) in plan.matches.iter().enumerate() {
            if !matches!(matched, Match::New) {
                continue;
            }
            let entry = &manifest.tracks[index];
            if !requested.insert((entry.hash.clone(), entry.extension.clone())) {
                continue;
            }
            let (complete, partial) = cache_paths(db, entry);
            if complete.is_file() {
                if fs::metadata(&complete)?.len() == entry.size
                    && hash_file(&complete, || self.active())? == entry.hash
                    && lofty::probe::Probe::open(&complete)?.read().is_ok()
                {
                    continue;
                }
                fs::remove_file(&complete)?;
            }
            let mut offset = fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
            if offset > entry.size {
                fs::remove_file(&partial)?;
                offset = 0;
            }
            missing.push(index);
            offsets.push(offset);
        }
        let total = missing.iter().map(|i| manifest.tracks[*i].size).sum();
        let resumed_bytes: u64 = offsets.iter().sum();
        let mut received = resumed_bytes;
        self.progress(&manifest.title, true, received, total, false);
        write_json(
            stream,
            &Reply {
                missing: missing.clone(),
                offsets: offsets.clone(),
                ..Reply::ok()
            },
        )?;
        for (index, offset) in missing.into_iter().zip(offsets) {
            let entry = &manifest.tracks[index];
            let (complete, partial) = cache_paths(db, entry);
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&partial)?;
            let mut remaining = entry.size - offset;
            let mut buffer = [0; 64 * 1024];
            while remaining > 0 {
                self.active()?;
                let limit = (remaining as usize).min(buffer.len());
                let size = stream.read(&mut buffer[..limit])?;
                ensure!(
                    size > 0,
                    "Передача прервалась. Повторите синхронизацию, чтобы продолжить"
                );
                file.write_all(&buffer[..size])?;
                remaining -= size as u64;
                received += size as u64;
                self.progress(&manifest.title, true, received, total, false);
            }
            file.sync_all()?;
            drop(file);
            if hash_file(&partial, || self.active())? != entry.hash {
                fs::remove_file(&partial)?;
                bail!("Контрольная сумма аудиофайла не совпала. Повторите передачу");
            }
            fs::rename(&partial, &complete)?;
            if let Err(error) = lofty::probe::Probe::open(&complete)?.read() {
                let _ = fs::remove_file(&complete);
                return Err(error).context("Получен некорректный аудиофайл");
            }
        }
        self.active()?;
        // Audio survives an interruption in the cache; visible library changes remain atomic.
        let mut created = Vec::<PathBuf>::new();
        let mut report = TransferReport {
            resumed_bytes,
            ..Default::default()
        };
        let result = (|| -> Result<()> {
            let mut conn = db.connect()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let playlist = if manifest.favorites || manifest.library {
                None
            } else if let Some(source) = &manifest.playlist {
                let mapped = tx.query_row("SELECT playlist_id FROM sync_playlists WHERE source_device=?1 AND source_playlist=?2", params![manifest.device,source], |r| r.get::<_,i64>(0)).optional()?;
                Some(if let Some(id) = mapped {
                    id
                } else {
                    tx.execute("INSERT INTO playlists(name) VALUES (?1)", [&manifest.title])?;
                    let id = tx.last_insert_rowid();
                    tx.execute(
                        "INSERT INTO sync_playlists VALUES (?1,?2,?3)",
                        params![manifest.device, source, id],
                    )?;
                    id
                })
            } else {
                None
            };
            let mut imported = Vec::new();
            for (index, entry) in manifest.tracks.iter().enumerate() {
                let id = match plan.matches[index] {
                    Match::Local(id) => {
                        report.reused += 1;
                        id
                    }
                    Match::Earlier(previous) => {
                        report.reused += 1;
                        imported[previous]
                    }
                    Match::New => {
                        let relative = format!("audio/{}.{}", Uuid::new_v4(), entry.extension);
                        let destination = db.root().join(&relative);
                        created.push(destination.clone());
                        fs::copy(cache_paths(db, entry).0, &destination)?;
                        OpenOptions::new()
                            .write(true)
                            .open(&destination)?
                            .sync_all()?;
                        tx.execute("INSERT INTO tracks(title,artist,source_url,file_path,duration_ms) VALUES (?1,?2,?3,?4,?5)", params![entry.title,entry.artist,entry.source_url,relative,entry.duration_ms])?;
                        let id = tx.last_insert_rowid();
                        tx.execute("INSERT INTO sync_tracks VALUES (?1,?2) ON CONFLICT(content_hash) DO UPDATE SET track_id=excluded.track_id", params![entry.hash,id])?;
                        let (size, modified) = file_stamp(&destination)?;
                        tx.execute(
                            "INSERT INTO sync_file_index VALUES (?1,?2,?3,?4)",
                            params![id, entry.hash, size, modified],
                        )?;
                        report.copied += 1;
                        id
                    }
                };
                imported.push(id);
                tx.execute("INSERT INTO sync_origins VALUES (?1,?2,?3,?4) ON CONFLICT(source_device,source_track,content_hash) DO UPDATE SET track_id=excluded.track_id", params![manifest.device,entry.source_id,entry.hash,id])?;
                if entry.favorite || manifest.favorites {
                    tx.execute(
                        "INSERT OR IGNORE INTO favorites(track_id) VALUES (?1)",
                        [id],
                    )?;
                }
                if let Some(playlist) = playlist {
                    tx.execute("INSERT OR IGNORE INTO playlist_tracks(playlist_id,track_id,position) SELECT ?1,?2,COALESCE(MAX(position)+1,0) FROM playlist_tracks WHERE playlist_id=?1", params![playlist,id])?;
                }
            }
            tx.execute(
                "INSERT INTO sync_receipts VALUES (?1,?2,?3)",
                params![
                    manifest.device,
                    manifest.transfer_id,
                    fingerprint(&manifest)?
                ],
            )?;
            tx.commit()?;
            Ok(())
        })();
        if result.is_err() {
            for path in created {
                let _ = fs::remove_file(path);
            }
        }
        result?;
        for entry in &manifest.tracks {
            let (complete, partial) = cache_paths(db, entry);
            let _ = fs::remove_file(complete);
            let _ = fs::remove_file(partial);
        }
        self.inner.status.lock().unwrap().revision += 1;
        self.progress(&manifest.title, true, total, total, true);
        write_json(
            stream,
            &Reply {
                report,
                ..Reply::ok()
            },
        )
    }
}

fn connect(target: &PairingCode) -> Result<TcpStream> {
    let stream = TcpStream::connect_timeout(&target.address, Duration::from_secs(8))
        .context("Устройство недоступно")?;
    configure(&stream)?;
    Ok(stream)
}
fn cache_paths(db: &Database, entry: &WireTrack) -> (PathBuf, PathBuf) {
    let name = format!("{}.{}", entry.hash, entry.extension);
    (
        db.root().join("sync-cache").join(&name),
        db.root().join("sync-cache").join(format!("{name}.part")),
    )
}
fn fingerprint(manifest: &Manifest) -> Result<String> {
    let mut stable = manifest.clone();
    stable.transfer_id.clear();
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&stable)?)
    ))
}
fn metadata_key(title: &str, artist: &str) -> Option<String> {
    fn normalize(text: &str) -> String {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }
    let title = normalize(title);
    let artist = normalize(artist);
    if title.is_empty() || artist.is_empty() {
        None
    } else {
        Some(format!("{title}\0{artist}"))
    }
}
fn durations_match(a: Option<i64>, b: Option<i64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.abs_diff(b) <= 3000,
        _ => false,
    }
}
fn file_stamp(path: &Path) -> Result<(i64, String)> {
    let metadata = fs::metadata(path)?;
    let modified = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos()
        .to_string();
    Ok((i64::try_from(metadata.len())?, modified))
}
fn indexed_hash(
    conn: &rusqlite::Connection,
    track: &Track,
    path: &Path,
    active: impl FnMut() -> Result<()>,
) -> Result<String> {
    let (size, modified) = file_stamp(path)?;
    let cached = conn.query_row("SELECT content_hash FROM sync_file_index WHERE track_id=?1 AND size=?2 AND modified=?3",params![track.id,size,modified],|r|r.get::<_,String>(0)).optional()?;
    if let Some(hash) = cached {
        return Ok(hash);
    }
    let hash = hash_file(path, active)?;
    conn.execute("INSERT INTO sync_file_index VALUES (?1,?2,?3,?4) ON CONFLICT(track_id) DO UPDATE SET content_hash=excluded.content_hash,size=excluded.size,modified=excluded.modified",params![track.id,hash,size,modified])?;
    Ok(hash)
}
