//! Seekable HTTP source: decode from the network without importing an audio file.
use anyhow::{Context, Result};
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::time::Duration;
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{Decoder as CodecDecoder, DecoderOptions},
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::{MediaSource, MediaSourceStream},
    meta::MetadataOptions,
    probe::Hint,
};

pub(super) struct HttpAudio {
    agent: ureq::Agent,
    url: String,
    body: Option<ureq::Body>,
    position: u64,
    length: u64,
    seekable: Arc<AtomicBool>,
}

impl HttpAudio {
    pub fn open(url: String) -> io::Result<Self> {
        Self::open_with_read_timeout(url, Duration::from_secs(15))
    }
    fn open_with_read_timeout(url: String, read_timeout: Duration) -> io::Result<Self> {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            // An audio body is consumed in real time. A total body deadline
            // would truncate every track after that deadline, even on fast Wi-Fi.
            .timeout_per_read(Some(read_timeout))
            .build()
            .into();
        let mut audio = Self {
            agent,
            url,
            body: None,
            position: 0,
            length: 0,
            seekable: Arc::new(AtomicBool::new(true)),
        };
        audio.request(0)?;
        Ok(audio)
    }
    fn request(&mut self, position: u64) -> io::Result<()> {
        let response = self
            .agent
            .get(&self.url)
            .header("Range", format!("bytes={position}-"))
            .call()
            .map_err(io::Error::other)?;
        if position != 0 && response.status().as_u16() != 206 {
            return Err(io::Error::other("Audio server does not support seeking"));
        }
        if response.status().as_u16() == 206 {
            let start = response
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("bytes "))
                .and_then(|v| v.split('-').next())
                .and_then(|v| v.parse::<u64>().ok());
            if start != Some(position) {
                return Err(io::Error::other(
                    "Audio server returned an incorrect byte range",
                ));
            }
        }
        if position == 0 {
            self.length = response
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.rsplit('/').next())
                .and_then(|v| v.parse().ok())
                .or_else(|| {
                    response
                        .headers()
                        .get("content-length")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse().ok())
                })
                .ok_or_else(|| io::Error::other("Audio server did not return content length"))?;
        }
        self.body = Some(response.into_body());
        self.position = position;
        Ok(())
    }
}

impl MediaSource for HttpAudio {
    fn is_seekable(&self) -> bool {
        self.seekable.load(Ordering::Relaxed)
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.length)
    }
}

/// Rodio 0.20's Read/Seek adapter discards the source length, which makes its
/// MP4 probe panic. Feed the HTTP source directly to Symphonia instead. Probe
/// progressively so playback starts before scanning every fragment of an album.
pub(super) struct StreamDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn CodecDecoder>,
    track: u32,
    samples: Vec<f32>,
    offset: usize,
    channels: u16,
    rate: u32,
    duration: Option<Duration>,
    error: Arc<Mutex<Option<String>>>,
    buffer: Option<SampleBuffer<f32>>,
}

impl StreamDecoder {
    pub fn open(url: String) -> Result<Self> {
        let audio = HttpAudio::open(url)?;
        let seekable = audio.seekable.clone();
        seekable.store(false, Ordering::Relaxed);
        let media = MediaSourceStream::new(Box::new(audio), Default::default());
        let probed = symphonia::default::get_probe().format(
            &Hint::new(),
            media,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )?;
        seekable.store(true, Ordering::Relaxed);
        let track = probed.format.default_track().context("No audio stream")?;
        let id = track.id;
        let params = &track.codec_params;
        let duration = params
            .time_base
            .zip(params.n_frames)
            .filter(|(_, frames)| *frames > 0)
            .map(|(base, frames)| {
                let time = base.calc_time(frames);
                Duration::from_secs(time.seconds) + Duration::from_secs_f64(time.frac)
            });
        let mut source = Self {
            decoder: symphonia::default::get_codecs().make(params, &DecoderOptions::default())?,
            track: id,
            channels: 0,
            rate: 0,
            duration,
            error: Arc::new(Mutex::new(None)),
            buffer: None,
            format: probed.format,
            samples: vec![],
            offset: 0,
        };
        source.read_packet()?;
        anyhow::ensure!(
            source.channels > 0 && source.rate > 0,
            "Invalid audio format"
        );
        Ok(source)
    }

    fn read_packet(&mut self) -> Result<u64> {
        let mut failures = 0;
        loop {
            let packet = self.format.next_packet()?;
            if packet.track_id() != self.track {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(symphonia::core::errors::Error::DecodeError(_)) if failures < 3 => {
                    failures += 1;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            self.channels = decoded.spec().channels.count() as u16;
            self.rate = decoded.spec().rate;
            if self
                .buffer
                .as_ref()
                .is_none_or(|b| b.capacity() < decoded.capacity() * self.channels as usize)
            {
                self.buffer = Some(SampleBuffer::<f32>::new(
                    decoded.capacity() as u64,
                    *decoded.spec(),
                ));
            }
            let buffer = self.buffer.as_mut().unwrap();
            buffer.copy_interleaved_ref(decoded);
            self.samples.clear();
            self.samples.extend_from_slice(buffer.samples());
            self.offset = 0;
            return Ok(packet.ts());
        }
    }
}
impl Iterator for StreamDecoder {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        while self.offset >= self.samples.len() {
            if let Err(error) = self.read_packet() {
                let eof = error.downcast_ref::<symphonia::core::errors::Error>().is_some_and(|e| matches!(e,
                    symphonia::core::errors::Error::IoError(e) if e.kind() == io::ErrorKind::UnexpectedEof));
                if !eof {
                    *self.error.lock().unwrap() = Some(error.to_string());
                }
                return None;
            }
        }
        let sample = self.samples[self.offset];
        self.offset += 1;
        Some(sample)
    }
}

/// Network reads and decoding stay on a producer thread. The audio callback
/// consumes at most 400 ms of queued PCM and emits silence while buffering.
/// Count only consumed audio samples so network stalls do not advance the clock.
#[derive(Default)]
pub(super) struct StreamStatus {
    pub consumed: AtomicU64,
    pub buffering: AtomicBool,
    pub error: Mutex<Option<String>>,
    cancel: AtomicBool,
}
pub(super) struct BufferedStream {
    packets: mpsc::Receiver<Vec<f32>>,
    packet: Vec<f32>,
    offset: usize,
    status: Arc<StreamStatus>,
    duration: Option<Duration>,
}
impl BufferedStream {
    pub fn open(
        url: String,
        position: Duration,
        duration: Option<Duration>,
    ) -> Result<(Self, Arc<StreamStatus>)> {
        use rodio::Source;
        let mut decoder = StreamDecoder::open(url)?;
        let duration = duration.or(decoder.duration);
        let position = duration
            .map(|d| position.min(d.saturating_sub(Duration::from_millis(1))))
            .unwrap_or(position);
        if !position.is_zero() {
            decoder
                .try_seek(position)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        }
        let error = decoder.error.clone();
        let normalized = rodio::source::UniformSourceIterator::<_, f32>::new(
            decoder,
            shard::desktop_audio::CHANNELS,
            shard::desktop_audio::RATE,
        );
        Self::spawn(normalized, duration, Some(error))
    }
    fn spawn(
        mut source: impl Iterator<Item = f32> + Send + 'static,
        duration: Option<Duration>,
        error: Option<Arc<Mutex<Option<String>>>>,
    ) -> Result<(Self, Arc<StreamStatus>)> {
        let status = Arc::new(StreamStatus::default());
        status.buffering.store(true, Ordering::Relaxed);
        let worker = status.clone();
        let (sender, packets) = mpsc::sync_channel(20);
        std::thread::Builder::new()
            .name("shard-stream-decoder".into())
            .spawn(move || {
                const SAMPLES: usize = (shard::desktop_audio::RATE as usize
                    * shard::desktop_audio::CHANNELS as usize)
                    / 50;
                while !worker.cancel.load(Ordering::Relaxed) {
                    let packet: Vec<_> = source.by_ref().take(SAMPLES).collect();
                    if packet.is_empty() {
                        if let Some(error) = &error {
                            *worker.error.lock().unwrap() = error.lock().unwrap().take();
                        }
                        break;
                    }
                    if sender.send(packet).is_err() {
                        break;
                    }
                }
            })?;
        Ok((
            Self {
                packets,
                packet: vec![],
                offset: 0,
                status: status.clone(),
                duration,
            },
            status,
        ))
    }
}
impl Iterator for BufferedStream {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.offset == self.packet.len() {
            self.status
                .consumed
                .fetch_add(self.packet.len() as u64, Ordering::Relaxed);
            self.packet.clear();
            self.offset = 0;
            match self.packets.try_recv() {
                Ok(packet) => {
                    self.packet = packet;
                    self.status.buffering.store(false, Ordering::Relaxed);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    self.status.buffering.store(true, Ordering::Relaxed);
                    return Some(0.);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.status.buffering.store(false, Ordering::Relaxed);
                    return None;
                }
            }
        }
        let sample = self.packet[self.offset];
        self.offset += 1;
        Some(sample)
    }
}
impl rodio::Source for BufferedStream {
    fn channels(&self) -> u16 {
        shard::desktop_audio::CHANNELS
    }
    fn sample_rate(&self) -> u32 {
        shard::desktop_audio::RATE
    }
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn total_duration(&self) -> Option<Duration> {
        self.duration
    }
}
impl Drop for BufferedStream {
    fn drop(&mut self) {
        self.status.cancel.store(true, Ordering::Relaxed);
    }
}
impl rodio::Source for StreamDecoder {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        self.duration
    }
    fn try_seek(
        &mut self,
        position: Duration,
    ) -> std::result::Result<(), rodio::source::SeekError> {
        let seek = (|| -> Result<()> {
            let position = self
                .duration
                .map(|d| position.min(d.saturating_sub(Duration::from_millis(1))))
                .unwrap_or(position);
            let result = self.format.seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: position.as_secs_f64().into(),
                    track_id: Some(self.track),
                },
            )?;
            self.decoder.reset();
            loop {
                let ts = self.read_packet()?;
                let track = self
                    .format
                    .tracks()
                    .iter()
                    .find(|t| t.id == self.track)
                    .context("Missing audio track")?;
                let base = track
                    .codec_params
                    .time_base
                    .context("Missing audio time base")?;
                let time = base.calc_time(result.required_ts.saturating_sub(ts));
                let skip = ((time.seconds as f64 + time.frac) * self.rate as f64) as usize
                    * self.channels as usize;
                if skip < self.samples.len() {
                    self.offset = skip;
                    break;
                }
            }
            Ok(())
        })();
        seek.map_err(|error| {
            rodio::source::SeekError::Other(Box::new(io::Error::other(error.to_string())))
        })
    }
}
impl Read for HttpAudio {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.length || buffer.is_empty() {
            return Ok(0);
        }
        for attempt in 0..3 {
            if self.body.is_none() {
                self.request(self.position)?;
            }
            match self.body.as_mut().unwrap().as_reader().read(buffer) {
                Ok(count) if count > 0 => {
                    self.position += count as u64;
                    return Ok(count);
                }
                // Resume interrupted connections at the last byte instead of
                // letting the decoder mistake a short response for track end.
                result => {
                    self.body = None;
                    if attempt == 2 {
                        return Err(io::Error::other(match result {
                            Err(error) => format!("Audio stream interrupted: {error}"),
                            _ => "Audio stream ended before its advertised length".into(),
                        }));
                    }
                }
            }
        }
        unreachable!()
    }
}
impl Seek for HttpAudio {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let next = match from {
            SeekFrom::Start(p) => i128::from(p),
            SeekFrom::Current(p) => i128::from(self.position) + i128::from(p),
            SeekFrom::End(p) => i128::from(self.length) + i128::from(p),
        };
        if next < 0 || next > i128::from(u64::MAX) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid audio seek",
            ));
        }
        let next = next as u64;
        if next != self.position {
            // Delay opening the next range until Read. Decoder probes often seek
            // several times without reading; these should not create requests.
            self.body = None;
            self.position = next;
        }
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        sync::mpsc,
    };

    #[test]
    fn audio_body_has_no_deadline_while_consumer_is_idle() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/audio", listener.local_addr().unwrap());
        let (resume, resumed) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\na")
                .unwrap();
            resumed.recv().unwrap();
            socket.write_all(b"b").unwrap();
        });
        let mut audio = HttpAudio::open_with_read_timeout(url, Duration::from_millis(250)).unwrap();
        let mut byte = [0];
        audio.read_exact(&mut byte).unwrap();
        assert_eq!(byte, *b"a");
        // Time spent by the player consuming buffered PCM must not eat the
        // next network read's timeout budget.
        std::thread::sleep(Duration::from_millis(500));
        resume.send(()).unwrap();
        audio.read_exact(&mut byte).unwrap();
        assert_eq!(byte, *b"b");
        server.join().unwrap();
    }

    #[test]
    fn buffered_callback_does_not_block_or_count_buffering_silence() {
        let (release, waiting) = mpsc::channel();
        let mut first = true;
        let source = std::iter::from_fn(move || {
            if first {
                waiting.recv().unwrap();
                first = false;
            }
            Some(0.5)
        })
        .take(1920);
        let (mut source, status) = BufferedStream::spawn(source, None, None).unwrap();
        let start = std::time::Instant::now();
        assert!(source.by_ref().take(9600).all(|v| v == 0.));
        assert!(start.elapsed() < Duration::from_millis(100));
        assert_eq!(status.consumed.load(Ordering::Relaxed), 0);
        assert!(status.buffering.load(Ordering::Relaxed));
        release.send(()).unwrap();
        let start = std::time::Instant::now();
        loop {
            if source.next() == Some(0.5) {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(2));
            std::thread::yield_now();
        }
        assert_eq!(source.by_ref().filter(|v| *v == 0.5).count(), 1919);
        assert_eq!(status.consumed.load(Ordering::Relaxed), 1920);
        assert!(!status.buffering.load(Ordering::Relaxed));
    }

    #[test]
    fn interrupted_transfer_resumes_from_last_consumed_byte() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/audio", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for offset in [0, 2] {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut range = String::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if line.to_ascii_lowercase().starts_with("range:") {
                        range = line;
                    }
                }
                assert_eq!(
                    range.trim().to_ascii_lowercase(),
                    format!("range: bytes={offset}-")
                );
                write!(socket, "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {offset}-3/4\r\nConnection: close\r\n\r\n", 4 - offset).unwrap();
                // Deliberately interrupt the first response after half its body.
                socket
                    .write_all(if offset == 0 { b"ab" } else { b"cd" })
                    .unwrap();
            }
        });
        let mut audio = HttpAudio::open(url).unwrap();
        let mut bytes = Vec::new();
        audio.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"abcd");
        server.join().unwrap();
    }

    #[test]
    fn fragmented_aac_decodes_and_seeks_without_downloading_a_file() {
        // Generated 440 Hz tone: FFmpeg AAC, empty_moov + dash fragmentation.
        let bytes = include_bytes!("../tests/fixtures/stream-fragmented.m4a");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/audio", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let server = std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let (mut socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut from = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        from = value.split('-').next().unwrap().parse().unwrap();
                    }
                }
                if write!(socket, "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n", bytes.len() - from, from, bytes.len() - 1, bytes.len()).is_ok() {
                    let _ = socket.write_all(&bytes[from..]);
                }
            }
        });
        use rodio::Source;
        let result = (|| -> Result<()> {
            let mut decoder = StreamDecoder::open(url)?;
            assert_eq!(decoder.channels(), 1);
            assert_eq!(decoder.sample_rate(), 8000);
            let samples: Vec<_> = decoder.by_ref().take(1024).collect();
            assert_eq!(samples.len(), 1024);
            assert!(samples.iter().any(|s| s.abs() > 0.01));
            decoder
                .try_seek(Duration::from_millis(250))
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            assert_eq!(decoder.by_ref().take(1024).count(), 1024);
            Ok(())
        })();
        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();
        result.unwrap();
    }

    #[test]
    #[ignore = "Requires YouTube network access and installed yt-dlp/FFmpeg"]
    fn live_youtube_audio_decodes_and_seeks() {
        use rodio::Source;
        use shard::youtube::search::{ResultKind, SearchResult, stream_url};
        let track = SearchResult {
            id: "qe8Q7mjxjig".into(),
            title: "Veridis Quo".into(),
            artist: "Daft Punk".into(),
            url: "https://www.youtube.com/watch?v=qe8Q7mjxjig".into(),
            thumbnail: None,
            duration_ms: Some(346000),
            play_count: None,
            kind: ResultKind::Track,
        };
        let url = stream_url(
            &std::env::current_dir().unwrap(),
            &track,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        let mut decoder = StreamDecoder::open(url).unwrap();
        let samples = decoder.channels() as usize * decoder.sample_rate() as usize;
        assert_eq!(
            decoder.by_ref().take(samples * 45).count(),
            samples * 45,
            "stream ended in the opening fragment"
        );
        decoder.try_seek(Duration::from_secs(30)).unwrap();
        assert_eq!(
            decoder.by_ref().take(samples * 45).count(),
            samples * 45,
            "stream ended after seeking"
        );
    }

    #[test]
    fn reads_before_transfer_finishes_and_reopens_only_when_read_after_seek() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/audio", listener.local_addr().unwrap());
        let (release, blocked) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let mut ranges = Vec::new();
            while ranges.len() < 2 && started.elapsed() < Duration::from_secs(10) {
                let (mut socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut from = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_lowercase().strip_prefix("range: bytes=") {
                        from = value.split('-').next().unwrap().parse().unwrap();
                    }
                }
                ranges.push(from);
                write!(socket, "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-1023/1024\r\nConnection: close\r\n\r\n", 1024 - from, from).unwrap();
                let bytes: Vec<_> = (from..1024).map(|p| (p % 251) as u8).collect();
                if ranges.len() == 1 {
                    socket.write_all(&bytes[..16]).unwrap();
                    socket.flush().unwrap();
                    blocked.recv_timeout(Duration::from_secs(3)).unwrap();
                    let _ = socket.write_all(&bytes[16..]);
                } else {
                    socket.write_all(&bytes).unwrap();
                }
            }
            ranges
        });
        let mut audio = HttpAudio::open(url).unwrap();
        let mut head = [0; 4];
        // This completes while the server is still waiting to send the rest.
        audio.read_exact(&mut head).unwrap();
        assert_eq!(head, [0, 1, 2, 3]);
        release.send(()).unwrap();
        assert!(audio.seek(SeekFrom::Current(-10)).is_err());
        assert_eq!(audio.seek(SeekFrom::End(0)).unwrap(), 1024);
        assert_eq!(audio.read(&mut head).unwrap(), 0);
        audio.seek(SeekFrom::Start(500)).unwrap();
        audio.seek(SeekFrom::Current(-100)).unwrap();
        audio.read_exact(&mut head).unwrap();
        assert_eq!(head, [149, 150, 151, 152]);
        assert_eq!(server.join().unwrap(), vec![0, 400]);
    }
}
