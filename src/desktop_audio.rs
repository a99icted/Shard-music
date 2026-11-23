//! Device-independent PCM playback and a bounded, loopback-only OBS media source.
use anyhow::{Context, Result};
use rodio::Source;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const RATE: u32 = 48_000;
pub const CHANNELS: u16 = 2;
pub const PACKET_SAMPLES: usize = 1_920; // 20 ms, stereo
const PACKET_BYTES: usize = PACKET_SAMPLES * 2;
pub const OBS_PORT: u16 = 17_863;
type Packet = Arc<[u8]>;

#[derive(Clone, Default)]
pub struct AudioBus {
    inner: Arc<BusInner>,
}
#[derive(Default)]
struct BusInner {
    listeners: Mutex<Vec<mpsc::SyncSender<Packet>>>,
    count: AtomicUsize,
}
impl AudioBus {
    fn subscribe(&self) -> mpsc::Receiver<Packet> {
        let (tx, rx) = mpsc::sync_channel(4);
        let mut listeners = self.inner.listeners.lock().unwrap();
        listeners.push(tx);
        self.inner.count.store(listeners.len(), Ordering::Relaxed);
        rx
    }
    fn publish(&self, bytes: &[u8]) {
        if self.inner.count.load(Ordering::Relaxed) == 0 {
            return;
        }
        // Never block an audio callback behind a network client or a settings action.
        let Ok(mut listeners) = self.inner.listeners.try_lock() else {
            return;
        };
        let packet: Packet = bytes.into();
        listeners.retain(|tx| match tx.try_send(packet.clone()) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => true,
            Err(mpsc::TrySendError::Disconnected(_)) => false,
        });
        // A scheduling hiccup may fill 80 ms of PCM. Drop the excess packet,
        // never the live connection; network write timeouts remove stalled clients.
        self.inner.count.store(listeners.len(), Ordering::Relaxed);
    }
    fn clear(&self) {
        self.inner.listeners.lock().unwrap().clear();
        self.inner.count.store(0, Ordering::Relaxed);
    }
    pub fn tap<S: Source<Item = f32>>(&self, source: S) -> AudioTap<S> {
        AudioTap {
            source,
            bus: self.clone(),
            bytes: [0; PACKET_BYTES],
            used: 0,
        }
    }
}

pub struct AudioTap<S> {
    source: S,
    bus: AudioBus,
    bytes: [u8; PACKET_BYTES],
    used: usize,
}
impl<S: Source<Item = f32>> Iterator for AudioTap<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let sample = self.source.next()?;
        if self.bus.inner.count.load(Ordering::Relaxed) != 0 {
            let pcm = (sample.clamp(-1., 1.) * i16::MAX as f32).round() as i16;
            self.bytes[self.used..self.used + 2].copy_from_slice(&pcm.to_le_bytes());
            self.used += 2;
            if self.used == PACKET_BYTES {
                self.bus.publish(&self.bytes);
                self.used = 0;
            }
        } else {
            self.used = 0;
        }
        Some(sample)
    }
}
impl<S: Source<Item = f32>> Source for AudioTap<S> {
    fn current_frame_len(&self) -> Option<usize> {
        self.source.current_frame_len()
    }
    fn channels(&self) -> u16 {
        self.source.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.source.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.source.total_duration()
    }
}

/// Consume real decoded samples at 48 kHz when no hardware callback is available.
/// The same Sink still handles pause, gain, end-of-track and sample-based position.
pub struct SoftwareOutput {
    stop: Arc<AtomicBool>,
    task: Option<JoinHandle<()>>,
}
impl SoftwareOutput {
    pub fn start(mut source: impl Source<Item = f32> + Send + 'static) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let task = thread::Builder::new()
            .name("shard-software-audio".into())
            .spawn(move || {
                let tick = Duration::from_millis(10);
                let mut deadline = Instant::now();
                while !cancelled.load(Ordering::Acquire) {
                    for _ in 0..(RATE as usize * CHANNELS as usize / 100) {
                        if source.next().is_none() {
                            return;
                        }
                    }
                    deadline += tick;
                    let now = Instant::now();
                    if deadline > now {
                        thread::park_timeout(deadline - now);
                    } else if now.duration_since(deadline) > Duration::from_millis(100) {
                        // A sleeping PC must not decode hours of audio in a catch-up burst.
                        deadline = now;
                    }
                }
            })?;
        Ok(Self {
            stop,
            task: Some(task),
        })
    }
}
impl Drop for SoftwareOutput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(task) = self.task.take() {
            task.thread().unpark();
            let _ = task.join();
        }
    }
}

pub struct ObsStream {
    stop: Arc<AtomicBool>,
    task: Option<JoinHandle<()>>,
    pub url: String,
}
impl ObsStream {
    pub fn start(bus: AudioBus, port: u16) -> Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
            .context("Could not open the local OBS audio stream")?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let task = thread::Builder::new()
            .name("shard-obs-audio".into())
            .spawn(move || {
                let mut clients: Vec<JoinHandle<()>> = vec![];
                while !cancelled.load(Ordering::Acquire) {
                    let mut i = 0;
                    while i < clients.len() {
                        if clients[i].is_finished() {
                            let _ = clients.swap_remove(i).join();
                        } else {
                            i += 1;
                        }
                    }
                    match listener.accept() {
                        Ok((stream, _)) if clients.len() < 4 => {
                            let bus = bus.clone();
                            let stop = cancelled.clone();
                            if let Ok(task) = thread::Builder::new()
                                .name("shard-obs-client".into())
                                .spawn(move || {
                                    let _ = serve(stream, bus, stop);
                                })
                            {
                                clients.push(task);
                            }
                        }
                        Ok((stream, _)) => {
                            drop(stream);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::park_timeout(Duration::from_millis(20))
                        }
                        Err(_) => break,
                    }
                }
                bus.clear();
                for task in clients {
                    let _ = task.join();
                }
            })?;
        Ok(Self {
            stop,
            task: Some(task),
            url: format!("http://127.0.0.1:{port}/audio.wav"),
        })
    }
}
impl Drop for ObsStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(task) = self.task.take() {
            task.thread().unpark();
            let _ = task.join();
        }
    }
}
fn wav_header() -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend(u32::MAX.to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(CHANNELS.to_le_bytes());
    bytes.extend(RATE.to_le_bytes());
    bytes.extend((RATE * CHANNELS as u32 * 2).to_le_bytes());
    bytes.extend((CHANNELS * 2).to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(u32::MAX.to_le_bytes());
    bytes
}
fn serve(mut stream: TcpStream, bus: AudioBus, stop: Arc<AtomicBool>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(400)))?;
    stream.set_write_timeout(Some(Duration::from_millis(200)))?;
    stream.set_nodelay(true)?;
    let mut request = vec![];
    let mut buf = [0; 512];
    while request.len() < 4096 && !request.windows(4).any(|w| w == b"\r\n\r\n") {
        let read = stream.read(&mut buf)?;
        if read == 0 {
            return Ok(());
        }
        request.extend_from_slice(&buf[..read]);
    }
    let get = request.starts_with(b"GET /audio.wav HTTP/1.");
    let head = request.starts_with(b"HEAD /audio.wav HTTP/1.");
    if !get && !head {
        stream.write_all(
            b"HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        )?;
        return Ok(());
    }
    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nCache-Control: no-store\r\nConnection: close\r\nAccept-Ranges: none\r\n\r\n")?;
    if head {
        return Ok(());
    }
    stream.write_all(&wav_header())?;
    let packets = bus.subscribe();
    let tick = Duration::from_millis(20);
    let mut deadline = Instant::now() + tick;
    while !stop.load(Ordering::Acquire) {
        let packet = match packets.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Ok(packet) => Some(packet),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        // One packet per deadline, including silence during idle playback. A late
        // callback must not add silence AND its packet to the same 20 ms slot.
        while Instant::now() < deadline && !stop.load(Ordering::Acquire) {
            thread::park_timeout(deadline.saturating_duration_since(Instant::now()));
        }
        if stop.load(Ordering::Acquire) {
            break;
        }
        if Instant::now().saturating_duration_since(deadline) > Duration::from_millis(100) {
            // Resume after system sleep without a burst or a broken OBS source.
            deadline = Instant::now();
        }
        match packet {
            Some(packet) => stream.write_all(&packet)?,
            None => stream.write_all(&[0; PACKET_BYTES])?,
        }
        deadline += tick;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires FFmpeg; validates the OBS Media Source demuxer"]
    fn ffmpeg_media_source_decodes_live_stream_without_an_audio_endpoint() {
        let root = tempfile::tempdir().unwrap();
        let ffmpeg = crate::download_tools::ffmpeg(root.path()).unwrap();
        let bus = AudioBus::default();
        let server = ObsStream::start(bus.clone(), 0).unwrap();
        let source = rodio::source::UniformSourceIterator::<_, f32>::new(
            rodio::source::SineWave::new(440.).amplify(0.3),
            CHANNELS,
            RATE,
        );
        let _output = SoftwareOutput::start(bus.tap(source)).unwrap();
        let path = root.path().join("captured.pcm");
        let mut child = std::process::Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-rw_timeout",
                "2000000",
                "-f",
                "wav",
                "-i",
            ])
            .arg(&server.url)
            .args(["-t", "0.3", "-f", "s16le", "-y"])
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > Duration::from_secs(10) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("FFmpeg did not decode the live stream");
            }
            thread::sleep(Duration::from_millis(20));
        };
        assert!(status.success());
        let pcm = std::fs::read(path).unwrap();
        assert_eq!(pcm.len(), RATE as usize * CHANNELS as usize * 2 * 3 / 10);
        assert!(
            pcm.chunks_exact(2)
                .any(|b| i16::from_le_bytes([b[0], b[1]]).abs() > 1000)
        );
    }
    #[test]
    fn slow_clients_do_not_block_audio_or_grow_buffers() {
        let bus = AudioBus::default();
        let slow = bus.subscribe();
        let fast = bus.subscribe();
        for _ in 0..10 {
            bus.publish(&[1; PACKET_BYTES]);
            assert_eq!(fast.recv().unwrap().len(), PACKET_BYTES);
        }
        assert_eq!(bus.inner.count.load(Ordering::Relaxed), 2);
        assert_eq!(slow.try_iter().count(), 4);
        drop(slow);
        bus.publish(&[1; PACKET_BYTES]);
        assert_eq!(bus.inner.count.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn http_stream_carries_decoded_pcm_without_a_device_and_releases_the_port() {
        let bus = AudioBus::default();
        let server = ObsStream::start(bus.clone(), 0).unwrap();
        let port: u16 = server
            .url
            .split(':')
            .nth(2)
            .unwrap()
            .split('/')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let mut client = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        client
            .write_all(b"GET /audio.wav HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut header = vec![];
        while !header.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            client.read_exact(&mut byte).unwrap();
            header.push(byte[0]);
        }
        assert!(header.starts_with(b"HTTP/1.1 200 OK"));
        let mut wav = [0; 44];
        client.read_exact(&mut wav).unwrap();
        assert_eq!(wav.as_slice(), wav_header());
        let source = rodio::source::SineWave::new(440.).amplify(0.3);
        let source = rodio::source::UniformSourceIterator::<_, f32>::new(source, CHANNELS, RATE);
        let output = SoftwareOutput::start(bus.tap(source)).unwrap();
        let mut pcm = [0; PACKET_BYTES * 3];
        client.read_exact(&mut pcm).unwrap();
        assert!(
            pcm.chunks_exact(2)
                .any(|b| i16::from_le_bytes([b[0], b[1]]).abs() > 1000)
        );
        let started = Instant::now();
        drop(output);
        drop(server);
        assert!(started.elapsed() < Duration::from_secs(2));
        TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
    }
}
