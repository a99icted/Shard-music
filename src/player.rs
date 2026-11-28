use anyhow::{Context, Result};
#[path = "player_stream.rs"]
mod stream;

use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::{Decoder, OutputStream, Sink, Source};
use shard::i18n::tr;
use shard::localized_format;
use shard::{
    db::Database,
    desktop_audio::{AudioBus, CHANNELS, OBS_PORT, ObsStream, RATE, SoftwareOutput},
    queue::{PlaybackQueue, RepeatMode},
    statistics::{ListeningMeter, StatisticsService},
    volume::amplitude,
};
use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub struct PlaybackTrack {
    pub id: i64,
    pub path: PathBuf,
}

enum Command {
    Start(Vec<PlaybackTrack>, i64),
    Stream(
        PathBuf,
        Vec<(i64, shard::youtube::search::SearchResult)>,
        i64,
        String,
    ),
    Library(Vec<PlaybackTrack>, bool),
    Ignored(std::collections::HashSet<i64>),
    Select(i64),
    Toggle,
    Stop,
    Next,
    Previous,
    Volume(f32),
    OutputDevice(Option<String>),
    RefreshDevices,
    Monitoring(bool),
    ObsStream(bool),
    #[cfg(test)]
    DeviceAvailable(bool),
    #[cfg(test)]
    LoseOutput,
    Shuffle(bool),
    Repeat(RepeatMode),
    Flush(mpsc::Sender<()>),
    Seek(Duration, u64, i64),
    Enqueue(PlaybackTrack),
    Remove(i64, Option<mpsc::Sender<()>>),
}

#[derive(Clone)]
pub struct PlaybackState {
    pub track_id: Option<i64>,
    pub paused: bool,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub buffering: bool,
    pub queue: Arc<PlaybackQueue>,
    pub error: Option<String>,
    pub output_devices: Vec<String>,
    pub active_output: Option<String>,
    pub device_less: bool,
    pub obs_url: Option<String>,
    seek_serial: u64,
    pending_seek: Option<u64>,
}

#[derive(Clone)]
pub struct Player {
    sender: mpsc::Sender<Command>,
    state: Arc<Mutex<PlaybackState>>,
    transport_observers: Arc<Mutex<Vec<async_channel::Sender<TransportState>>>>,
    change_observers: Arc<Mutex<Vec<async_channel::Sender<()>>>>,
    stream_cancel: Arc<AtomicBool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportState {
    pub has_track: bool,
    pub paused: bool,
}

impl PlaybackState {
    fn publish_position(&mut self, track: Option<i64>, actual_ms: u64, seek_ack: Option<u64>) {
        if self.track_id != track || self.pending_seek == seek_ack {
            self.pending_seek = None;
        }
        self.track_id = track;
        if self.pending_seek.is_none() {
            self.position_ms = actual_ms;
        }
    }
    fn transport(&self) -> TransportState {
        TransportState {
            has_track: self.track_id.is_some(),
            paused: self.paused,
        }
    }
}

struct Engine {
    output: Option<(OutputStream, rodio::OutputStreamHandle)>,
    sink: Option<Sink>,
    software_output: Option<SoftwareOutput>,
    monitoring: bool,
    audio_bus: AudioBus,
    obs_stream: Option<ObsStream>,
    #[cfg(test)]
    device_available: bool,
    tracks: HashMap<i64, PathBuf>,
    stream: HashMap<i64, shard::youtube::search::SearchResult>,
    stream_urls: HashMap<i64, (String, Instant)>,
    stream_status: Option<Arc<stream::StreamStatus>>,
    stream_root: PathBuf,
    stream_cancel: Arc<AtomicBool>,
    library: Vec<PlaybackTrack>,
    autoplay: bool,
    queue: PlaybackQueue,
    volume: f32,
    error: Option<String>,
    statistics: Option<StatisticsWriter>,
    meter: ListeningMeter,
    measured_at: Instant,
    duration_ms: Option<u64>,
    session: String,
    pending_ms: u64,
    pending_at: i64,
    output_device: Option<String>,
    active_output: Option<String>,
    device_checked_at: Instant,
    position_offset: Duration,
    recovery: Option<(Duration, bool)>,
    watchdog: PlaybackWatchdog,
}

#[derive(Default)]
struct PlaybackWatchdog {
    position: Duration,
    unchanged: Duration,
}
impl PlaybackWatchdog {
    fn sample(&mut self, position: Duration, elapsed: Duration, active: bool) -> bool {
        if !active || position != self.position {
            self.unchanged = Duration::ZERO;
        } else {
            self.unchanged += elapsed;
        }
        self.position = position;
        self.unchanged >= Duration::from_secs(5)
    }
}

fn output_devices() -> Vec<String> {
    let mut names: Vec<_> = rodio::cpal::default_host()
        .output_devices()
        .into_iter()
        .flatten()
        .filter_map(|device| device.name().ok())
        .collect();
    names.sort();
    names.dedup();
    names
}

enum StatisticsCommand {
    Record {
        session: String,
        track: i64,
        ms: u64,
        at: i64,
    },
    Flush(mpsc::Sender<()>),
}
struct StatisticsWriter(mpsc::Sender<StatisticsCommand>);
impl StatisticsWriter {
    fn new(service: StatisticsService, state: Arc<Mutex<PlaybackState>>) -> Self {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("shard-statistics".into())
            .spawn(move || {
                while let Ok(command) = receiver.recv() {
                    match command {
                        StatisticsCommand::Record {
                            session,
                            track,
                            ms,
                            at,
                        } => {
                            if let Err(error) = service.record(&session, track, ms, at) {
                                state.lock().unwrap().error = Some(localized_format!(
                                    "Не удалось сохранить статистику: {error:#}",
                                    "Could not save statistics: {error:#}"
                                ));
                            }
                        }
                        StatisticsCommand::Flush(reply) => {
                            let _ = reply.send(());
                        }
                    }
                }
            })
            .expect("statistics worker");
        Self(sender)
    }
    fn flush(&self, reply: mpsc::Sender<()>) {
        let _ = self.0.send(StatisticsCommand::Flush(reply));
    }
}

impl Engine {
    fn measure(&mut self) {
        let elapsed = self.measured_at.elapsed().as_millis() as u64;
        self.measured_at = Instant::now();
        if let Some(sink) = &self.sink {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            self.pending_ms += self.meter.sample(
                (self.position_offset + sink.get_pos()).as_millis() as u64,
                elapsed,
                sink.is_paused(),
                sink.empty(),
                self.duration_ms,
            );
            if self.pending_ms > 0 {
                self.pending_at = now;
            }
            if self.pending_ms >= 1000 {
                self.flush_statistics();
            }
        }
    }
    fn flush_statistics(&mut self) {
        if let Some(service) = &self.statistics
            && let Some(id) = self.queue.current()
            && id > 0
            && self.pending_ms > 0
            && let Err(error) = service.0.send(StatisticsCommand::Record {
                session: self.session.clone(),
                track: id,
                ms: self.pending_ms,
                at: self.pending_at,
            })
        {
            self.error = Some(localized_format!(
                "Не удалось сохранить статистику: {error:#}",
                "Could not save statistics: {error:#}"
            ));
        }
        self.pending_ms = 0;
    }

    fn extend_queue(&mut self) -> bool {
        let ids: Vec<_> = self.library.iter().map(|t| t.id).collect();
        if self
            .queue
            .extend_from_library(&ids, self.autoplay && self.stream.is_empty())
        {
            for track in &self.library {
                self.tracks.insert(track.id, track.path.clone());
            }
            true
        } else {
            false
        }
    }

    fn position(&self) -> Duration {
        self.recovery.map(|r| r.0).unwrap_or_else(|| {
            if let Some(status) = &self.stream_status {
                return self.position_offset
                    + Duration::from_secs_f64(
                        status.consumed.load(Ordering::Relaxed) as f64
                            / (RATE as f64 * CHANNELS as f64),
                    );
            }
            self.sink
                .as_ref()
                .map(|s| self.position_offset + s.get_pos())
                .unwrap_or_default()
        })
    }

    fn open_output(&mut self) -> Result<()> {
        if self.output.is_some() || !self.monitoring {
            return Ok(());
        }
        #[cfg(test)]
        if !self.device_available {
            return Ok(());
        }
        let host = rodio::cpal::default_host();
        let selected = self.output_device.as_ref().and_then(|name| {
            host.output_devices()
                .ok()?
                .find(|device| device.name().ok().as_ref() == Some(name))
        });
        let selected = selected.and_then(|device| {
            OutputStream::try_from_device(&device)
                .ok()
                .map(|stream| (stream, device.name().ok()))
        });
        let available = selected.or_else(|| {
            let device = host.default_output_device()?;
            OutputStream::try_from_device(&device)
                .ok()
                .map(|stream| (stream, device.name().ok()))
        });
        if let Some((stream, name)) = available {
            self.active_output = name;
            self.output = Some(stream);
        }
        // No endpoint is a supported playback mode. Real decoded PCM is consumed
        // by SoftwareOutput, and remains available to the optional OBS stream.

        Ok(())
    }

    fn load_current(&mut self, position: Duration, paused: bool) -> Result<()> {
        let id = self.queue.current().context("No current track")?;
        let mut source: Box<dyn Source<Item = f32> + Send> = if let Some(result) =
            self.stream.get_mut(&id)
        {
            if self
                .stream_urls
                .get(&id)
                .is_some_and(|(_, at)| at.elapsed() >= Duration::from_secs(300))
            {
                self.stream_urls.remove(&id);
            }
            if !self.stream_urls.contains_key(&id) {
                let info = shard::youtube::search::resolve_stream(
                    &self.stream_root,
                    result,
                    &self.stream_cancel,
                )?;
                result.duration_ms = info.duration_ms.or(result.duration_ms);
                self.stream_urls.insert(id, (info.url, Instant::now()));
            }
            let duration = result
                .duration_ms
                .filter(|ms| *ms > 0)
                .map(|ms| Duration::from_millis(ms as u64));
            let (source, status) =
                stream::BufferedStream::open(self.stream_urls[&id].0.clone(), position, duration)?;
            self.stream_status = Some(status);
            Box::new(source)
        } else {
            let path = self
                .tracks
                .get(&id)
                .context(tr("Трек отсутствует в очереди"))?;
            Box::new(
                Decoder::new(BufReader::new(
                    File::open(path).context(tr("Аудиофайл не найден"))?,
                ))
                .context(tr("Не удалось декодировать аудио"))?
                .convert_samples::<f32>(),
            )
        };
        self.duration_ms = source
            .total_duration()
            .map(|d| d.as_millis() as u64)
            .or_else(|| {
                self.stream
                    .get(&id)
                    .and_then(|r| r.duration_ms)
                    .map(|ms| ms.max(0) as u64)
            });
        let position = self
            .duration_ms
            .map(|ms| position.min(Duration::from_millis(ms)))
            .unwrap_or(position);
        // Seek the decoder on this worker. Sink::try_seek waits for the audio callback
        // forever when an output device has died, preventing even Stop from working.
        if !position.is_zero() && self.stream_status.is_none() {
            source
                .try_seek(position)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        }
        let (sink, queue) = Sink::new_idle();
        sink.set_volume(amplitude(self.volume));
        if paused {
            sink.pause();
        }
        sink.append(source);
        let source = rodio::source::UniformSourceIterator::<_, f32>::new(queue, CHANNELS, RATE);
        let source = self.audio_bus.tap(source);
        if let Some((_, output)) = &self.output {
            output.play_raw(source)?;
        } else {
            self.software_output = Some(SoftwareOutput::start(source)?);
        }
        self.sink = Some(sink);
        self.position_offset = position;
        self.meter.reset_position(position.as_millis() as u64);
        self.measured_at = Instant::now();
        self.watchdog = PlaybackWatchdog::default();
        self.recovery = None;
        Ok(())
    }

    fn stop_sink(&mut self) {
        // Stop the consumer before replacing its queue; at most one software
        // consumer can run and produce PCM for OBS at a time.
        self.software_output = None;
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.stream_status = None;
    }

    fn play_current(&mut self) {
        self.stop_sink();
        self.recovery = None;
        self.position_offset = Duration::ZERO;
        self.session = uuid::Uuid::new_v4().to_string();
        while self.queue.current().is_some() {
            // Device failures must retain the queue, unlike a missing/corrupt file.
            if let Err(error) = self.open_output() {
                self.error = Some(format!("{error:#}"));
                self.recovery = Some((Duration::ZERO, false));
                self.watchdog = PlaybackWatchdog::default();
                break;
            }
            match self.load_current(Duration::ZERO, false) {
                Ok(()) => {
                    break;
                }
                Err(error) => {
                    if !self.stream.is_empty() && self.stream_cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    self.error = Some(format!("{error:#}"));
                    self.queue.skip_failed();
                }
            }
        }
    }

    fn restart_output(&mut self, position: Duration, paused: bool) {
        self.stop_sink();
        self.output = None;
        self.active_output = None;
        self.recovery = Some((position, paused));
        self.watchdog = PlaybackWatchdog::default();
        if self.queue.current().is_none() {
            self.recovery = None;
            return;
        }
        if let Err(error) = self
            .open_output()
            .and_then(|_| self.load_current(position, paused))
        {
            self.error = Some(localized_format!(
                "Не удалось восстановить аудио: {error:#}",
                "Could not restore audio: {error:#}"
            ));
        }
    }

    fn seek(&mut self, position: Duration) {
        let paused = self
            .recovery
            .map(|r| r.1)
            .unwrap_or_else(|| self.sink.as_ref().is_some_and(Sink::is_paused));
        if self.queue.current().is_none() {
            return;
        }
        self.stop_sink();
        if let Err(error) = self
            .open_output()
            .and_then(|_| self.load_current(position, paused))
        {
            self.error = Some(localized_format!(
                "Не удалось перемотать трек: {error:#}",
                "Could not seek track: {error:#}"
            ));
            self.recovery = Some((position, paused));
        }
    }
    fn remove(&mut self, id: i64) {
        let current_removed = self.queue.remove(id);
        self.tracks.remove(&id);
        if current_removed {
            self.play_current();
        }
    }
}

impl Player {
    pub fn new(volume: f32, shuffle: bool, database: Option<Database>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let mut queue = PlaybackQueue::default();
        queue.set_shuffle(shuffle);
        let state = Arc::new(Mutex::new(PlaybackState {
            track_id: None,
            paused: false,
            position_ms: 0,
            duration_ms: None,
            buffering: false,
            queue: Arc::new(queue.clone()),
            error: None,
            output_devices: Vec::new(),
            active_output: None,
            device_less: false,
            obs_url: None,
            seek_serial: 0,
            pending_seek: None,
        }));
        let shared = state.clone();
        let statistics =
            database.map(|db| StatisticsWriter::new(StatisticsService::new(db), shared.clone()));
        let change_observers = Arc::new(Mutex::new(Vec::<async_channel::Sender<()>>::new()));
        let updates = change_observers.clone();
        let transport_observers = Arc::new(Mutex::new(
            Vec::<async_channel::Sender<TransportState>>::new(),
        ));
        let observers = transport_observers.clone();
        let stream_cancel = Arc::new(AtomicBool::new(false));
        let resolving_cancel = stream_cancel.clone();
        std::thread::spawn(move || {
            let mut engine = Engine {
                output: None,
                sink: None,
                software_output: None,
                monitoring: true,
                audio_bus: AudioBus::default(),
                obs_stream: None,
                #[cfg(test)]
                device_available: true,
                tracks: HashMap::new(),
                stream: HashMap::new(),
                stream_urls: HashMap::new(),
                stream_status: None,
                stream_root: PathBuf::new(),
                stream_cancel: resolving_cancel,
                library: Vec::new(),
                autoplay: true,
                queue,
                volume,
                error: None,
                statistics,
                meter: ListeningMeter::default(),
                measured_at: Instant::now(),
                duration_ms: None,
                session: String::new(),
                pending_ms: 0,
                pending_at: 0,
                output_device: None,
                active_output: None,
                device_checked_at: Instant::now(),
                position_offset: Duration::ZERO,
                recovery: None,
                watchdog: PlaybackWatchdog::default(),
            };
            loop {
                let mut acknowledge = None;
                let mut seek_ack = None;
                let mut flush_statistics = false;
                let command = if engine.recovery.is_none()
                    && engine.sink.as_ref().is_none_or(Sink::is_paused)
                {
                    if engine.monitoring && engine.sink.is_some() {
                        // Reconnect devices while paused without resuming the track.
                        receiver.recv_timeout(Duration::from_secs(2))
                    } else {
                        receiver
                            .recv()
                            .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                    }
                } else {
                    receiver.recv_timeout(Duration::from_millis(100))
                };
                let mut queue_changed = matches!(
                    &command,
                    Ok(Command::Start(..)
                        | Command::Stream(..)
                        | Command::Select(..)
                        | Command::Stop
                        | Command::Next
                        | Command::Previous
                        | Command::Shuffle(..)
                        | Command::Repeat(..)
                        | Command::Enqueue(..)
                        | Command::Remove(..))
                        | Ok(Command::Ignored(..))
                );
                let mut notify = command.is_ok();
                let elapsed = engine.measured_at.elapsed();
                engine.measure();
                let active = engine.recovery.is_some()
                    || engine
                        .sink
                        .as_ref()
                        .is_some_and(|sink| !sink.is_paused() && !sink.empty());
                let position = engine.position();
                let buffering = engine
                    .stream_status
                    .as_ref()
                    .is_some_and(|s| s.buffering.load(Ordering::Relaxed));
                let stalled = engine
                    .watchdog
                    .sample(position, elapsed, active && !buffering);
                // Flush before any command can change the current track or seek position.
                if !matches!(command, Err(mpsc::RecvTimeoutError::Timeout)) {
                    engine.flush_statistics();
                }
                if matches!(
                    &command,
                    Ok(Command::Start(..)
                        | Command::Stream(..)
                        | Command::Select(..)
                        | Command::Stop
                        | Command::Next
                        | Command::Previous)
                ) {
                    engine.stream_cancel.store(false, Ordering::Relaxed);
                }
                match command {
                    Ok(Command::Ignored(ids)) => {
                        if engine
                            .queue
                            .current()
                            .is_some_and(|id| ids.contains(&id) && !engine.queue.is_ignored(id))
                        {
                            let current = engine.queue.current();
                            for id in &ids {
                                if Some(*id) != current {
                                    engine.queue.remove(*id);
                                }
                            }
                            let pool: Vec<_> = engine
                                .library
                                .iter()
                                .filter(|track| !ids.contains(&track.id))
                                .map(|track| track.id)
                                .collect();
                            if engine.queue.extend_from_library(&pool, engine.autoplay) {
                                for track in &engine.library {
                                    engine.tracks.insert(track.id, track.path.clone());
                                }
                            }
                        }
                        if engine.queue.set_ignored(ids) {
                            engine.play_current();
                        }
                    }
                    Ok(Command::Library(tracks, autoplay)) => {
                        engine.library = tracks;
                        engine.autoplay = autoplay;
                    }
                    Ok(Command::Start(tracks, selected)) => {
                        engine.stream.clear();
                        engine.stream_urls.clear();
                        engine
                            .queue
                            .start(tracks.iter().map(|track| track.id), selected);
                        engine.tracks = tracks
                            .into_iter()
                            .map(|track| (track.id, track.path))
                            .collect();
                        engine.play_current();
                    }
                    Ok(Command::Stream(root, tracks, id, url)) => {
                        engine.queue.start(tracks.iter().map(|t| t.0), id);
                        engine.tracks.clear();
                        engine.stream = tracks.into_iter().collect();
                        engine.stream_urls.clear();
                        engine.stream_urls.insert(id, (url, Instant::now()));
                        engine.stream_root = root;
                        engine.play_current();
                    }
                    Ok(Command::Select(id)) => {
                        if engine.queue.select(id) {
                            engine.play_current();
                        }
                    }
                    Ok(Command::Toggle) => {
                        if let Some((_, paused)) = &mut engine.recovery {
                            *paused = !*paused;
                        } else if let Some(sink) = &engine.sink {
                            if sink.is_paused() {
                                sink.play();
                            } else {
                                sink.pause();
                            }
                        }
                    }
                    Ok(Command::Stop) => {
                        engine.stop_sink();
                        engine.recovery = None;
                        engine.output = None;
                        engine.active_output = None;
                        engine.position_offset = Duration::ZERO;
                        engine.queue.clear();
                        engine.tracks.clear();
                    }
                    Ok(Command::Next) => {
                        engine.extend_queue();
                        engine.queue.advance();
                        engine.play_current();
                    }
                    Ok(Command::Previous) => {
                        if engine.position() > Duration::from_secs(3) {
                            engine.seek(Duration::ZERO);
                        } else {
                            engine.queue.previous();
                            engine.play_current();
                        }
                    }
                    Ok(Command::RefreshDevices) => {
                        shared.lock().unwrap().output_devices = output_devices();
                    }
                    Ok(Command::Monitoring(enabled)) => {
                        if engine.monitoring != enabled {
                            engine.monitoring = enabled;
                            let position = engine.position();
                            let paused = engine.recovery.map(|r| r.1).unwrap_or_else(|| {
                                engine.sink.as_ref().is_some_and(Sink::is_paused)
                            });
                            engine.restart_output(position, paused);
                        }
                    }
                    Ok(Command::ObsStream(enabled)) => {
                        if enabled && engine.obs_stream.is_none() {
                            match ObsStream::start(engine.audio_bus.clone(), OBS_PORT) {
                                Ok(stream) => engine.obs_stream = Some(stream),
                                Err(error) => engine.error = Some(format!("{error:#}")),
                            }
                        } else if !enabled {
                            engine.obs_stream = None;
                        }
                    }
                    #[cfg(test)]
                    Ok(Command::DeviceAvailable(available)) => {
                        engine.device_available = available;
                        let position = engine.position();
                        let paused = engine.sink.as_ref().is_some_and(Sink::is_paused);
                        engine.restart_output(position, paused);
                    }
                    #[cfg(test)]
                    Ok(Command::LoseOutput) => {
                        // Simulate a terminated device callback with an attached sink.
                        engine.output = None;
                    }
                    Ok(Command::OutputDevice(device)) => {
                        let position = engine.position();
                        let paused = engine
                            .recovery
                            .map(|r| r.1)
                            .unwrap_or_else(|| engine.sink.as_ref().is_some_and(Sink::is_paused));
                        engine.output_device = device;
                        engine.restart_output(position, paused);
                        shared.lock().unwrap().output_devices = output_devices();
                    }
                    Ok(Command::Volume(volume)) => {
                        engine.volume = volume;
                        if let Some(sink) = &engine.sink {
                            sink.set_volume(amplitude(volume));
                        }
                    }
                    Ok(Command::Shuffle(shuffle)) => engine.queue.set_shuffle(shuffle),
                    Ok(Command::Repeat(repeat)) => engine.queue.set_repeat(repeat),
                    Ok(Command::Flush(reply)) => {
                        acknowledge = Some(reply);
                        flush_statistics = true;
                    }
                    Ok(Command::Seek(position, serial, track)) => {
                        if engine.queue.current() == Some(track) {
                            engine.seek(position);
                        }
                        seek_ack = Some(serial);
                    }
                    Ok(Command::Enqueue(track)) => {
                        let was_idle = engine.queue.current().is_none();
                        if engine.queue.enqueue(track.id) {
                            engine.tracks.insert(track.id, track.path);
                        }
                        if was_idle {
                            engine.play_current();
                        }
                    }
                    Ok(Command::Remove(id, reply)) => {
                        if reply.is_some() {
                            engine.library.retain(|track| track.id != id);
                        }
                        engine.remove(id);
                        acknowledge = reply;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                if engine.monitoring
                    && engine.sink.is_some()
                    && engine.device_checked_at.elapsed() >= Duration::from_secs(2)
                {
                    engine.device_checked_at = Instant::now();
                    if engine.output.is_none() && engine.software_output.is_some() {
                        // Keep consuming PCM while checking. Failed attempts must
                        // neither rewind nor recreate the software stream.
                        if engine.open_output().is_ok() && engine.output.is_some() {
                            let position = engine.position();
                            let paused = engine.sink.as_ref().is_some_and(Sink::is_paused);
                            engine.stop_sink();
                            if let Err(error) = engine.load_current(position, paused) {
                                engine.error = Some(format!("{error:#}"));
                                engine.recovery = Some((position, paused));
                            }
                            notify = true;
                        }
                    } else if engine.output_device.is_none() {
                        let default = rodio::cpal::default_host()
                            .default_output_device()
                            .and_then(|device| device.name().ok());
                        if default != engine.active_output {
                            let position = engine.position();
                            let paused = engine.sink.as_ref().is_some_and(Sink::is_paused);
                            engine.restart_output(position, paused);
                            notify = true;
                        }
                    }
                }
                if stalled && !notify {
                    let paused = engine.recovery.map(|r| r.1).unwrap_or(false);
                    engine.flush_statistics();
                    engine.restart_output(position, paused);
                    notify = true;
                }
                // Advance here, independently of UI redraw timing.
                if engine.sink.as_ref().is_some_and(Sink::empty) {
                    if let Some(status) = &engine.stream_status {
                        if let Some(error) = status.error.lock().unwrap().take() {
                            engine.error = Some(format!("YouTube: {error}"));
                        }
                    }
                    engine.flush_statistics();
                    engine.extend_queue();
                    engine.queue.finish_track();
                    engine.play_current();
                    queue_changed = true;
                    notify = true;
                }
                {
                    let mut state = shared.lock().unwrap();
                    let previous_transport = state.transport();
                    let duration = engine.queue.current().and(engine.duration_ms);
                    let buffering = engine
                        .stream_status
                        .as_ref()
                        .is_some_and(|s| s.buffering.load(Ordering::Relaxed));
                    notify |= state.duration_ms != duration || state.buffering != buffering;
                    state.duration_ms = duration;
                    state.buffering = buffering;
                    state.publish_position(
                        engine.queue.current(),
                        engine.position().as_millis() as u64,
                        seek_ack,
                    );
                    if queue_changed {
                        state.queue = Arc::new(engine.queue.clone());
                    }
                    state.paused = engine
                        .recovery
                        .map(|r| r.1)
                        .unwrap_or_else(|| engine.sink.as_ref().is_some_and(Sink::is_paused));
                    state.active_output = engine.active_output.clone();
                    state.device_less = engine.software_output.is_some();
                    state.obs_url = engine.obs_stream.as_ref().map(|stream| stream.url.clone());
                    if let Some(error) = engine.error.take() {
                        state.error = Some(error);
                    }
                    let transport = state.transport();
                    if transport != previous_transport {
                        // Only transport changes wake desktop integrations, never position ticks.
                        observers
                            .lock()
                            .unwrap()
                            .retain(|sender| sender.try_send(transport).is_ok());
                    }
                }
                if let Some(reply) = acknowledge {
                    if flush_statistics && let Some(statistics) = &engine.statistics {
                        // Preserve shutdown durability without blocking playback behind SQLite.
                        statistics.flush(reply);
                    } else {
                        let _ = reply.send(());
                    }
                }
                if notify {
                    updates
                        .lock()
                        .unwrap()
                        .retain(|sender| match sender.try_send(()) {
                            Ok(()) | Err(async_channel::TrySendError::Full(_)) => true,
                            Err(async_channel::TrySendError::Closed(_)) => false,
                        });
                }
            }
        });
        Self {
            sender,
            state,
            transport_observers,
            change_observers,
            stream_cancel,
        }
    }
    pub fn set_library(&self, tracks: Vec<PlaybackTrack>, autoplay: bool) {
        let _ = self.sender.send(Command::Library(tracks, autoplay));
    }
    pub fn set_ignored(&self, ids: std::collections::HashSet<i64>) {
        let _ = self.sender.send(Command::Ignored(ids));
    }
    pub fn start(&self, tracks: Vec<PlaybackTrack>, selected: i64) {
        self.stream_cancel.store(true, Ordering::Relaxed);
        let _ = self.sender.send(Command::Start(tracks, selected));
    }
    pub fn stream(
        &self,
        root: PathBuf,
        tracks: Vec<(i64, shard::youtube::search::SearchResult)>,
        id: i64,
        url: String,
    ) {
        self.stream_cancel.store(true, Ordering::Relaxed);
        let _ = self.sender.send(Command::Stream(root, tracks, id, url));
    }
    pub fn select(&self, id: i64) {
        self.stream_cancel.store(true, Ordering::Relaxed);
        let _ = self.sender.send(Command::Select(id));
    }
    pub fn toggle(&self) {
        let _ = self.sender.send(Command::Toggle);
    }
    pub fn stop(&self) {
        self.stream_cancel.store(true, Ordering::Relaxed);
        let _ = self.sender.send(Command::Stop);
    }
    pub fn next(&self) {
        self.stream_cancel.store(true, Ordering::Relaxed);
        let _ = self.sender.send(Command::Next);
    }
    pub fn previous(&self) {
        self.stream_cancel.store(true, Ordering::Relaxed);
        let _ = self.sender.send(Command::Previous);
    }
    pub fn set_monitoring(&self, enabled: bool) {
        let _ = self.sender.send(Command::Monitoring(enabled));
    }
    pub fn set_obs_stream(&self, enabled: bool) {
        let _ = self.sender.send(Command::ObsStream(enabled));
    }
    pub fn set_output_device(&self, device: Option<String>) {
        let _ = self.sender.send(Command::OutputDevice(device));
    }
    pub fn refresh_output_devices(&self) {
        let _ = self.sender.send(Command::RefreshDevices);
    }
    pub fn set_volume(&self, volume: f32) {
        if volume.is_finite() {
            let _ = self.sender.send(Command::Volume(volume.clamp(0., 1.)));
        }
    }
    pub fn set_shuffle(&self, shuffle: bool) {
        let _ = self.sender.send(Command::Shuffle(shuffle));
    }
    pub fn set_repeat(&self, repeat: RepeatMode) {
        let _ = self.sender.send(Command::Repeat(repeat));
    }
    pub fn flush_and_wait(&self) {
        let (sender, receiver) = mpsc::channel();
        if self.sender.send(Command::Flush(sender)).is_ok() {
            let _ = receiver.recv_timeout(Duration::from_secs(2));
        }
    }
    pub fn seek(&self, milliseconds: u64) {
        let mut state = self.state.lock().unwrap();
        let Some(track) = state.track_id else { return };
        state.seek_serial = state.seek_serial.wrapping_add(1);
        let serial = state.seek_serial;
        // Publish the target before releasing the drag preview. Worker ticks and older
        // seek acknowledgements must never replace it with a stale decoder position.
        state.pending_seek = Some(serial);
        state.position_ms = milliseconds;
        if self
            .sender
            .send(Command::Seek(
                Duration::from_millis(milliseconds),
                serial,
                track,
            ))
            .is_err()
        {
            state.pending_seek = None;
        }
    }
    pub fn enqueue(&self, track: PlaybackTrack) {
        let _ = self.sender.send(Command::Enqueue(track));
    }
    pub fn remove(&self, id: i64) {
        let _ = self.sender.send(Command::Remove(id, None));
    }
    pub fn remove_and_wait(&self, id: i64) -> Result<()> {
        let (sender, receiver) = mpsc::channel();
        self.sender
            .send(Command::Remove(id, Some(sender)))
            .context(tr("Аудиоплеер недоступен"))?;
        receiver
            .recv_timeout(Duration::from_secs(5))
            .context(tr("Аудиоплеер не ответил на удаление трека"))?;
        Ok(())
    }
    pub fn state(&self) -> PlaybackState {
        self.state.lock().unwrap().clone()
    }
    pub fn progress(&self) -> (Option<i64>, bool, u64) {
        let state = self.state.lock().unwrap();
        (state.track_id, state.paused, state.position_ms)
    }
    pub fn timing(&self) -> (Option<i64>, u64, Option<u64>) {
        let state = self.state.lock().unwrap();
        (state.track_id, state.position_ms, state.duration_ms)
    }
    pub fn subscribe_transport(&self) -> async_channel::Receiver<TransportState> {
        let (sender, receiver) = async_channel::unbounded();
        // Keep the same lock order as publishing, so the initial state cannot overtake an update.
        let state = self.state.lock().unwrap();
        let _ = sender.try_send(state.transport());
        self.transport_observers.lock().unwrap().push(sender);
        receiver
    }
    pub fn subscribe_changes(&self) -> async_channel::Receiver<()> {
        let (sender, receiver) = async_channel::bounded(1);
        self.change_observers.lock().unwrap().push(sender);
        receiver
    }
    pub fn take_error(&self) -> Option<String> {
        self.state.lock().unwrap().error.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    #[ignore = "Requires YouTube network access and installed yt-dlp/FFmpeg; plays muted for 22 seconds"]
    fn live_youtube_progress_survives_body_deadline_and_supports_pause_seek() {
        use shard::youtube::search::{ResultKind, SearchResult, resolve_stream};
        let root = std::env::current_dir().unwrap();
        let mut track = SearchResult {
            id: "qe8Q7mjxjig".into(),
            title: "Veridis Quo".into(),
            artist: "Daft Punk".into(),
            url: "https://www.youtube.com/watch?v=qe8Q7mjxjig".into(),
            thumbnail: None,
            duration_ms: None,
            play_count: None,
            kind: ResultKind::Track,
        };
        let info = resolve_stream(&root, &track, &AtomicBool::new(false)).unwrap();
        assert!(info.duration_ms.is_some_and(|d| d > 60_000));
        track.duration_ms = info.duration_ms;
        let player = Player::new(0., false, None);
        player.sender.send(Command::DeviceAvailable(false)).unwrap();
        player.stream(root, vec![(-1, track)], -1, info.url);
        wait(&player, |s| s.track_id == Some(-1) && s.position_ms > 100);
        let start = Instant::now();
        loop {
            let state = player.state();
            assert!(state.error.is_none(), "{:?}", state.error);
            assert_eq!(state.track_id, Some(-1));
            assert!(state.duration_ms.is_some_and(|d| d > 60_000));
            if state.position_ms >= 22_000 {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(40),
                "Stream stopped at {} ms",
                state.position_ms
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        player.toggle();
        wait(&player, |s| s.paused);
        let paused = player.progress().2;
        std::thread::sleep(Duration::from_millis(300));
        assert!(player.progress().2.abs_diff(paused) <= 40);
        player.seek(60_000);
        wait(&player, |s| {
            s.pending_seek.is_none() && s.position_ms.abs_diff(60_000) <= 40
        });
        assert!(player.state().paused);
        player.toggle();
        wait(&player, |s| s.position_ms > 60_500 && !s.paused);
        eprintln!(
            "YouTube playback: duration {:?} ms, position {} ms after seek; pause and 22-second real-time playback passed",
            player.timing().2,
            player.progress().2
        );
        player.stop();
        wait(&player, |s| s.track_id.is_none());
    }

    fn test_audio(root: &std::path::Path, seconds: u32) -> PathBuf {
        let rate = 8000u32;
        let data_len = rate * seconds * 2;
        let mut wav = b"RIFF".to_vec();
        wav.extend((data_len + 36).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(rate.to_le_bytes());
        wav.extend((rate * 2).to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(data_len.to_le_bytes());
        for sample in 0..rate * seconds {
            let pcm = ((sample as f32 * 440. * std::f32::consts::TAU / rate as f32).sin() * 16000.)
                as i16;
            wav.extend(pcm.to_le_bytes());
        }
        let path = root.join("tone.wav");
        std::fs::write(&path, wav).unwrap();
        path
    }

    fn stream_client(url: &str) -> std::net::TcpStream {
        let address = url
            .strip_prefix("http://")
            .unwrap()
            .split('/')
            .next()
            .unwrap();
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .write_all(b"GET /audio.wav HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut header = vec![];
        while !header.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            header.push(byte[0]);
        }
        assert!(header.starts_with(b"HTTP/1.1 200 OK"));
        let mut wav = [0; 44];
        stream.read_exact(&mut wav).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        stream
    }

    fn read_peak(stream: &mut std::net::TcpStream, milliseconds: usize) -> i16 {
        let mut pcm = vec![0; milliseconds * 48 * 2 * 2];
        stream.read_exact(&mut pcm).unwrap();
        pcm.chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]).saturating_abs())
            .max()
            .unwrap()
    }

    fn wait_stream_peak(stream: &mut std::net::TcpStream, audible: bool) {
        // Commands can leave earlier PCM in this test client's TCP receive
        // buffer. Consume it as OBS does, then inspect consecutive current slots.
        let mut matching = 0;
        let started = Instant::now();
        while matching < 5 {
            let peak = read_peak(stream, 20);
            matching = if (peak > 5000) == audible {
                matching + 1
            } else {
                0
            };
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "Unexpected stream amplitude: {peak}"
            );
        }
    }

    #[test]
    fn real_pcm_without_devices_supports_obs_pause_gain_seeks_and_queue_transitions() {
        let root = tempfile::tempdir().unwrap();
        let source = test_audio(root.path(), 6);
        let player = Player::new(0.8, false, None);
        player.sender.send(Command::DeviceAvailable(false)).unwrap();
        player.set_obs_stream(true);
        player.start(
            (1..=2)
                .map(|id| PlaybackTrack {
                    id,
                    path: source.clone(),
                })
                .collect(),
            1,
        );
        wait(&player, |s| {
            s.device_less && s.obs_url.is_some() && s.position_ms > 100
        });
        assert!(player.state().active_output.is_none());
        let mut stream = stream_client(player.state().obs_url.as_ref().unwrap());
        let started = Instant::now();
        assert!(
            read_peak(&mut stream, 400) > 5000,
            "OBS must receive actual decoded audio"
        );
        assert!(
            (Duration::from_millis(300)..Duration::from_millis(800)).contains(&started.elapsed()),
            "PCM must be paced in real time"
        );
        player.set_volume(0.);
        player.flush_and_wait();
        read_peak(&mut stream, 200); // Drain in-flight PCM.
        assert_eq!(read_peak(&mut stream, 100), 0);
        player.set_volume(0.8);
        player.toggle();
        wait(&player, |s| s.paused);
        let paused = player.state().position_ms;
        read_peak(&mut stream, 200);
        assert_eq!(read_peak(&mut stream, 100), 0);
        player.flush_and_wait();
        assert!(player.state().position_ms.abs_diff(paused) <= 15);
        for target in [4000, 1000, 5000, 2000, 3000] {
            player.seek(target);
            assert_eq!(player.progress().2, target);
        }
        wait(&player, |s| {
            s.pending_seek.is_none() && s.position_ms.abs_diff(3000) < 15
        });
        player.set_output_device(Some("Missing output for deterministic test".into()));
        player.flush_and_wait();
        assert!(player.state().paused && player.state().device_less);
        player.toggle();
        wait(&player, |s| s.position_ms >= 3100 && !s.paused);
        wait_stream_peak(&mut stream, true);
        player.seek(5700);
        wait(&player, |s| s.track_id == Some(2) && s.position_ms >= 100);
        assert!(player.state().device_less);
        // A new source must continue using the existing OBS connection.
        wait_stream_peak(&mut stream, true);
        player.set_repeat(RepeatMode::One);
        player.seek(5700);
        wait(&player, |s| {
            s.pending_seek.is_none() && s.position_ms < 1000
        });
        assert_eq!(player.state().track_id, Some(2));
        player.set_repeat(RepeatMode::Off);
        player.seek(5700);
        wait(&player, |s| s.track_id.is_none());
        wait_stream_peak(&mut stream, false);
        player.set_obs_stream(false);
        player.flush_and_wait();
        assert!(player.state().obs_url.is_none());
    }

    #[test]
    #[ignore = "requires a working audio output; reconnect test uses muted generated audio"]
    fn reconnecting_output_preserves_software_position_and_pause() {
        let root = tempfile::tempdir().unwrap();
        let source = test_audio(root.path(), 6);
        let player = Player::new(0., false, None);
        player.sender.send(Command::DeviceAvailable(false)).unwrap();
        player.start(
            vec![PlaybackTrack {
                id: 1,
                path: source,
            }],
            1,
        );
        wait(&player, |s| s.device_less && s.position_ms > 300);
        player.toggle();
        wait(&player, |s| s.paused);
        let position = player.state().position_ms;
        player.sender.send(Command::DeviceAvailable(true)).unwrap();
        wait(&player, |s| !s.device_less && s.active_output.is_some());
        assert!(player.state().paused);
        assert!(player.state().position_ms.abs_diff(position) < 25);
        player.toggle();
        wait(&player, |s| s.position_ms > position + 200);
        player.set_monitoring(false);
        wait(&player, |s| s.device_less && s.active_output.is_none());
        player.stop();
        wait(&player, |s| s.track_id.is_none());
    }

    fn wait(player: &Player, predicate: impl Fn(&PlaybackState) -> bool) {
        let started = Instant::now();
        loop {
            let state = player.state();
            assert!(state.error.is_none(), "Playback error: {:?}", state.error);
            if predicate(&state) {
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(12),
                "Playback did not recover/respond"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    #[ignore = "requires a working audio output; uses only generated silence"]
    fn recovers_lost_output_and_seeks_without_waiting_for_dead_callback() {
        let root = tempfile::tempdir().unwrap();
        let data_len = 8000u32 * 2 * 20;
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&(data_len + 36).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        for value in [1u16, 1] {
            wav.extend_from_slice(&value.to_le_bytes());
        }
        for value in [8000u32, 16000] {
            wav.extend_from_slice(&value.to_le_bytes());
        }
        for value in [2u16, 16] {
            wav.extend_from_slice(&value.to_le_bytes());
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.resize(44 + data_len as usize, 0);
        let source = root.path().join("silence.wav");
        std::fs::write(&source, wav).unwrap();
        let player = Player::new(0., false, None);
        player.start(
            vec![
                PlaybackTrack {
                    id: 1,
                    path: source.clone(),
                },
                PlaybackTrack {
                    id: 2,
                    path: source,
                },
            ],
            1,
        );
        wait(&player, |s| s.position_ms >= 200);
        let position = player.state().position_ms;
        player.sender.send(Command::LoseOutput).unwrap();
        player.flush_and_wait();
        // No command or app restart is needed for the watchdog to restore the stream.
        wait(&player, |s| s.position_ms > position + 300);
        assert_eq!(player.state().track_id, Some(1));
        assert_eq!(player.state().queue.upcoming(), &[2]);
        player.sender.send(Command::LoseOutput).unwrap();
        player.seek(5000);
        wait(&player, |s| s.position_ms >= 5000);
        player.toggle();
        wait(&player, |s| s.paused);
        for target in [9000, 2000, 14000, 7000, 12000] {
            player.seek(target);
            assert_eq!(player.progress().2, target);
        }
        wait(&player, |s| {
            s.pending_seek.is_none() && s.position_ms.abs_diff(12000) < 50
        });
        let settled = player.progress().2;
        std::thread::sleep(Duration::from_millis(250));
        assert!(player.progress().2.abs_diff(settled) < 25);
        player.stop();
        wait(&player, |s| s.track_id.is_none());
    }

    #[test]
    fn optimistic_seeks_reject_stale_ticks_and_acknowledgements() {
        let player = Player::new(0., false, None);
        let mut state = player.state();
        state.track_id = Some(1);
        state.position_ms = 12_000;
        state.pending_seek = Some(2);
        state.publish_position(Some(1), 100, None);
        assert_eq!(state.position_ms, 12_000);
        state.publish_position(Some(1), 5_000, Some(1));
        assert_eq!(state.position_ms, 12_000);
        state.publish_position(Some(1), 12_010, Some(2));
        assert_eq!(state.position_ms, 12_010);
        assert!(state.pending_seek.is_none());
        state.pending_seek = Some(3);
        state.publish_position(Some(2), 0, None);
        assert_eq!(state.position_ms, 0);
        assert!(state.pending_seek.is_none());
    }

    #[test]
    fn watchdog_recovers_stalls_but_ignores_pause_seeks_and_long_playback() {
        let mut watchdog = PlaybackWatchdog::default();
        let tick = Duration::from_millis(100);
        // More than three hours of continuous playback without a wall-clock wait.
        for step in 1..=144_000 {
            assert!(!watchdog.sample(tick * step, tick, true));
        }
        let position = Duration::from_secs(14_400);
        for _ in 0..49 {
            assert!(!watchdog.sample(position, tick, true));
        }
        assert!(watchdog.sample(position, tick, true));
        assert!(!watchdog.sample(position, Duration::from_secs(3600), false));
        assert!(!watchdog.sample(Duration::from_secs(80), tick, true));
        assert!(!watchdog.sample(Duration::from_secs(80), tick, true));
    }
}
