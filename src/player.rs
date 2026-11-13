use anyhow::{Context, Result};
use rodio::{Decoder, OutputStream, Sink, Source};
use shard::{
    db::Database,
    queue::{PlaybackQueue, RepeatMode},
    statistics::{ListeningMeter, StatisticsService},
    volume::amplitude,
};
use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::PathBuf,
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
    Select(i64),
    Toggle,
    Stop,
    Next,
    Previous,
    Volume(f32),
    Shuffle(bool),
    Repeat(RepeatMode),
    Flush(mpsc::Sender<()>),
    Seek(Duration),
    Enqueue(PlaybackTrack),
    Remove(i64, Option<mpsc::Sender<()>>),
}

#[derive(Clone)]
pub struct PlaybackState {
    pub track_id: Option<i64>,
    pub paused: bool,
    pub position_ms: u64,
    pub queue: PlaybackQueue,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct Player {
    sender: mpsc::Sender<Command>,
    state: Arc<Mutex<PlaybackState>>,
    transport_observers: Arc<Mutex<Vec<async_channel::Sender<TransportState>>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportState {
    pub has_track: bool,
    pub paused: bool,
}

impl PlaybackState {
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
    tracks: HashMap<i64, PathBuf>,
    queue: PlaybackQueue,
    volume: f32,
    error: Option<String>,
    statistics: Option<StatisticsService>,
    meter: ListeningMeter,
    measured_at: Instant,
    duration_ms: Option<u64>,
    session: String,
    pending_ms: u64,
    pending_at: i64,
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
                sink.get_pos().as_millis() as u64,
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
            && self.pending_ms > 0
            && let Err(error) = service.record(&self.session, id, self.pending_ms, self.pending_at)
        {
            self.error = Some(format!("Не удалось сохранить статистику: {error:#}"));
        }
        self.pending_ms = 0;
    }

    fn play_current(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        while let Some(id) = self.queue.current() {
            let result = (|| -> Result<Sink> {
                if self.output.is_none() {
                    self.output =
                        Some(OutputStream::try_default().context("Аудиоустройство недоступно")?);
                }
                let path = self.tracks.get(&id).context("Трек отсутствует в очереди")?;
                let source = Decoder::new(BufReader::new(
                    File::open(path).context("Аудиофайл не найден")?,
                ))
                .context("Не удалось декодировать аудио")?;
                self.duration_ms = source.total_duration().map(|d| d.as_millis() as u64);
                let sink = Sink::try_new(&self.output.as_ref().unwrap().1)?;
                sink.set_volume(amplitude(self.volume));
                sink.append(source);
                Ok(sink)
            })();
            match result {
                Ok(sink) => {
                    self.sink = Some(sink);
                    self.session = uuid::Uuid::new_v4().to_string();
                    self.meter.reset_position(0);
                    self.measured_at = Instant::now();
                    break;
                }
                Err(error) => {
                    self.error = Some(format!("{error:#}"));
                    self.queue.skip_failed();
                }
            }
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
        let statistics = database.map(StatisticsService::new);
        let (sender, receiver) = mpsc::channel();
        let mut queue = PlaybackQueue::default();
        queue.set_shuffle(shuffle);
        let state = Arc::new(Mutex::new(PlaybackState {
            track_id: None,
            paused: false,
            position_ms: 0,
            queue: queue.clone(),
            error: None,
        }));
        let shared = state.clone();
        let transport_observers = Arc::new(Mutex::new(
            Vec::<async_channel::Sender<TransportState>>::new(),
        ));
        let observers = transport_observers.clone();
        std::thread::spawn(move || {
            let mut engine = Engine {
                output: None,
                sink: None,
                tracks: HashMap::new(),
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
            };
            loop {
                let mut acknowledge = None;
                let command = if engine.sink.as_ref().is_none_or(Sink::is_paused) {
                    // Idle and paused playback needs no periodic audio-worker wakeups.
                    receiver
                        .recv()
                        .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                } else {
                    receiver.recv_timeout(Duration::from_millis(100))
                };
                let mut queue_changed = matches!(
                    &command,
                    Ok(Command::Start(..)
                        | Command::Select(..)
                        | Command::Stop
                        | Command::Next
                        | Command::Previous
                        | Command::Shuffle(..)
                        | Command::Repeat(..)
                        | Command::Enqueue(..)
                        | Command::Remove(..))
                );
                engine.measure();
                // Flush before any command can change the current track or seek position.
                if !matches!(command, Err(mpsc::RecvTimeoutError::Timeout)) {
                    engine.flush_statistics();
                }
                match command {
                    Ok(Command::Start(tracks, selected)) => {
                        engine
                            .queue
                            .start(tracks.iter().map(|track| track.id), selected);
                        engine.tracks = tracks
                            .into_iter()
                            .map(|track| (track.id, track.path))
                            .collect();
                        engine.play_current();
                    }
                    Ok(Command::Select(id)) => {
                        if engine.queue.select(id) {
                            engine.play_current();
                        }
                    }
                    Ok(Command::Toggle) => {
                        if let Some(sink) = &engine.sink {
                            if sink.is_paused() {
                                sink.play();
                            } else {
                                sink.pause();
                            }
                        }
                    }
                    Ok(Command::Stop) => {
                        if let Some(sink) = engine.sink.take() {
                            sink.stop();
                        }
                        engine.queue.clear();
                        engine.tracks.clear();
                    }
                    Ok(Command::Next) => {
                        engine.queue.advance();
                        engine.play_current();
                    }
                    Ok(Command::Previous) => {
                        if let Some(sink) = &engine.sink
                            && sink.get_pos() > Duration::from_secs(3)
                        {
                            if let Err(error) = sink.try_seek(Duration::ZERO) {
                                engine.error = Some(format!("Не удалось перемотать трек: {error}"));
                            }
                            engine
                                .meter
                                .reset_position(sink.get_pos().as_millis() as u64);
                        } else {
                            engine.queue.previous();
                            engine.play_current();
                        }
                    }
                    Ok(Command::Volume(volume)) => {
                        engine.volume = volume;
                        if let Some(sink) = &engine.sink {
                            sink.set_volume(amplitude(volume));
                        }
                    }
                    Ok(Command::Shuffle(shuffle)) => engine.queue.set_shuffle(shuffle),
                    Ok(Command::Repeat(repeat)) => engine.queue.set_repeat(repeat),
                    Ok(Command::Flush(reply)) => acknowledge = Some(reply),
                    Ok(Command::Seek(position)) => {
                        if let Some(sink) = &engine.sink {
                            if let Err(error) = sink.try_seek(position) {
                                engine.error = Some(format!("Не удалось перемотать трек: {error}"));
                            }
                            engine
                                .meter
                                .reset_position(sink.get_pos().as_millis() as u64);
                        }
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
                        engine.remove(id);
                        acknowledge = reply;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                // Advance here, independently of UI redraw timing.
                if engine.sink.as_ref().is_some_and(Sink::empty) {
                    engine.flush_statistics();
                    engine.queue.finish_track();
                    engine.play_current();
                    queue_changed = true;
                }
                {
                    let mut state = shared.lock().unwrap();
                    let previous_transport = state.transport();
                    state.track_id = engine.queue.current();
                    if queue_changed {
                        state.queue = engine.queue.clone();
                    }
                    state.paused = engine.sink.as_ref().is_some_and(Sink::is_paused);
                    state.position_ms = engine
                        .sink
                        .as_ref()
                        .map(|sink| sink.get_pos().as_millis() as u64)
                        .unwrap_or(0);
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
                    let _ = reply.send(());
                }
            }
        });
        Self {
            sender,
            state,
            transport_observers,
        }
    }
    pub fn start(&self, tracks: Vec<PlaybackTrack>, selected: i64) {
        let _ = self.sender.send(Command::Start(tracks, selected));
    }
    pub fn select(&self, id: i64) {
        let _ = self.sender.send(Command::Select(id));
    }
    pub fn toggle(&self) {
        let _ = self.sender.send(Command::Toggle);
    }
    pub fn stop(&self) {
        let _ = self.sender.send(Command::Stop);
    }
    pub fn next(&self) {
        let _ = self.sender.send(Command::Next);
    }
    pub fn previous(&self) {
        let _ = self.sender.send(Command::Previous);
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
        let _ = self
            .sender
            .send(Command::Seek(Duration::from_millis(milliseconds)));
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
            .context("Аудиоплеер недоступен")?;
        receiver
            .recv_timeout(Duration::from_secs(5))
            .context("Аудиоплеер не ответил на удаление трека")?;
        Ok(())
    }
    pub fn state(&self) -> PlaybackState {
        self.state.lock().unwrap().clone()
    }
    pub fn progress(&self) -> (Option<i64>, bool, u64) {
        let state = self.state.lock().unwrap();
        (state.track_id, state.paused, state.position_ms)
    }
    pub fn subscribe_transport(&self) -> async_channel::Receiver<TransportState> {
        let (sender, receiver) = async_channel::unbounded();
        // Keep the same lock order as publishing, so the initial state cannot overtake an update.
        let state = self.state.lock().unwrap();
        let _ = sender.try_send(state.transport());
        self.transport_observers.lock().unwrap().push(sender);
        receiver
    }
    pub fn take_error(&self) -> Option<String> {
        self.state.lock().unwrap().error.take()
    }
}
