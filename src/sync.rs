//! Versioned, explicit LAN transfers. Pairing codes also serve as mobile QR payloads.
//! Session credentials rotate whenever the listener starts; peers are not auto-trusted on restart.
use crate::{
    db::Database,
    favorites::{FAVORITES_ID, FavoritesService},
    models::Track,
    repositories::TrackRepository,
    services::{AUDIO_EXTENSIONS, PlaylistService, TrackService},
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;

mod transfer;
pub use transfer::{
    DuplicateDecision, DuplicateKind, DuplicateTrack, SyncOptions, SyncSelection, TransferReport,
};

const VERSION: u32 = 2;
const MAX_JSON: usize = 8 * 1024 * 1024;
const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_TOTAL: u64 = 32 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct TransferProgress {
    pub title: String,
    pub receiving: bool,
    pub bytes: u64,
    pub total: u64,
    pub complete: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncStatus {
    pub peer: Option<String>,
    pub progress: Option<TransferProgress>,
    pub error: Option<String>,
    /// Changes only when received data is committed, so UI refreshes once per transfer.
    pub revision: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Peer {
    device: String,
    code: String,
    name: String,
}
#[derive(Clone, Debug)]
struct PairingCode {
    address: SocketAddr,
    token: String,
}
impl PairingCode {
    fn parse(input: &str) -> Result<Self> {
        let url = url::Url::parse(input.trim()).context("Некорректный код подключения")?;
        ensure!(
            url.scheme() == "shard-sync"
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "Некорректный код подключения"
        );
        let ip: IpAddr = url
            .host_str()
            .context("В коде нет адреса устройства")?
            .trim_matches(['[', ']'])
            .parse()?;
        ensure!(
            local_address(ip),
            "Подключение разрешено только в локальной сети"
        );
        let token = url.path().trim_start_matches('/');
        ensure!(
            Uuid::parse_str(token).is_ok(),
            "Некорректный код подключения"
        );
        let port = url.port().context("В коде нет порта устройства")?;
        ensure!(port > 0, "Некорректный порт устройства");
        Ok(Self {
            address: SocketAddr::new(ip, port),
            token: token.into(),
        })
    }
}
fn local_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private() || shared_address(ip) || ip.is_loopback() || ip.is_link_local()
        }
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local(),
    }
}
fn shared_address(ip: Ipv4Addr) -> bool {
    // RFC 6598: providers also use this range on directly connected LANs.
    ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1])
}

#[derive(Clone, Serialize, Deserialize)]
struct WireTrack {
    source_id: String,
    title: String,
    artist: String,
    source_url: Option<String>,
    duration_ms: Option<i64>,
    extension: String,
    hash: String,
    size: u64,
    favorite: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Manifest {
    transfer_id: String,
    library: bool,
    version: u32,
    device: String,
    playlist: Option<String>,
    title: String,
    favorites: bool,
    tracks: Vec<WireTrack>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request {
    Pair {
        version: u32,
        token: String,
        peer: Peer,
    },
    Inspect {
        token: String,
        manifest: Manifest,
    },
    Transfer {
        token: String,
        manifest: Manifest,
        copies: Vec<usize>,
    },
}
#[derive(Serialize, Deserialize)]
struct Reply {
    ok: bool,
    error: Option<String>,
    peer: Option<Peer>,
    missing: Vec<usize>,
    offsets: Vec<u64>,
    duplicates: Vec<DuplicateTrack>,
    done: bool,
    report: TransferReport,
}
impl Reply {
    fn ok() -> Self {
        Self {
            ok: true,
            error: None,
            peer: None,
            missing: vec![],
            offsets: vec![],
            duplicates: vec![],
            done: false,
            report: TransferReport::default(),
        }
    }
    fn check(self) -> Result<Self> {
        ensure!(
            self.ok,
            "{}",
            self.error
                .as_deref()
                .unwrap_or("Устройство отклонило запрос")
        );
        Ok(self)
    }
}
fn write_json(stream: &mut impl Write, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(
        bytes.len() <= MAX_JSON,
        "Слишком большой запрос синхронизации"
    );
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&bytes)?;
    Ok(())
}
fn read_json<T: for<'a> Deserialize<'a>>(stream: &mut impl Read) -> Result<T> {
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    ensure!(size <= MAX_JSON, "Слишком большой запрос синхронизации");
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn configure(stream: &TcpStream) -> Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    Ok(())
}

struct Inner {
    db: Database,
    device: String,
    name: String,
    token: String,
    code: String,
    peer: Mutex<Option<Peer>>,
    status: Mutex<SyncStatus>,
    stop: AtomicBool,
    transferring: AtomicBool,
    listener: Mutex<Option<TcpListener>>,
    sockets: Mutex<HashMap<u64, TcpStream>>,
    next_socket: AtomicU64,
    worker: Mutex<Option<std::thread::Thread>>,
}
#[derive(Clone)]
pub struct SyncService {
    inner: Arc<Inner>,
}
struct TransferGuard<'a>(&'a AtomicBool);
struct SocketGuard {
    inner: Arc<Inner>,
    id: u64,
}
// Windows shutdown() closes the connection for the peer, but a recv already in
// progress on another duplicated handle may still wait for its socket timeout.
// Short OS waits plus logical deadlines allow cancellation without truncating frames.
struct CancellableStream {
    stream: TcpStream,
    guard: SocketGuard,
    read_timeout: Option<Duration>,
}
impl CancellableStream {
    fn set_read_timeout(&mut self, timeout: Duration) {
        self.read_timeout = Some(timeout);
    }
    fn retry<T>(
        &self,
        timeout: Option<Duration>,
        mut operation: impl FnMut() -> std::io::Result<T>,
    ) -> std::io::Result<T> {
        let started = std::time::Instant::now();
        loop {
            if self.guard.inner.stop.load(Ordering::Acquire) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionAborted,
                    "Синхронизация выключена",
                ));
            }
            match operation() {
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) && timeout.is_none_or(|limit| started.elapsed() < limit) => {}
                result => return result,
            }
        }
    }
}
impl Read for CancellableStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.retry(self.read_timeout, || (&self.stream).read(buffer))
    }
}
impl Write for CancellableStream {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.retry(Some(Duration::from_secs(30)), || {
            (&self.stream).write(buffer)
        })
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Drop for SocketGuard {
    fn drop(&mut self) {
        self.inner.sockets.lock().unwrap().remove(&self.id);
    }
}
impl Drop for TransferGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl SyncService {
    pub fn start(db: Database, device: String, name: String) -> Result<Self> {
        let ip = local_ip();
        Self::listen(
            db,
            device,
            name,
            SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0),
            ip,
        )
    }
    fn listen(
        db: Database,
        device: String,
        name: String,
        bind: SocketAddr,
        advertised: IpAddr,
    ) -> Result<Self> {
        let listener = TcpListener::bind(bind).context("Не удалось включить синхронизацию")?;
        listener.set_nonblocking(true)?;
        let token = Uuid::new_v4().to_string();
        let code = format!(
            "shard-sync://{}/{token}",
            SocketAddr::new(advertised, listener.local_addr()?.port())
        );
        let service = Self {
            inner: Arc::new(Inner {
                db,
                device,
                name,
                token,
                code,
                peer: Mutex::new(None),
                status: Mutex::new(SyncStatus::default()),
                stop: AtomicBool::new(false),
                transferring: AtomicBool::new(false),
                listener: Mutex::new(Some(listener)),
                sockets: Mutex::new(HashMap::new()),
                next_socket: AtomicU64::new(0),
                worker: Mutex::new(None),
            }),
        };
        let weak = Arc::downgrade(&service.inner);
        let worker = std::thread::Builder::new()
            .name("shard-lan-sync".into())
            .spawn(move || {
                while let Some(inner) = weak.upgrade() {
                    if inner.stop.load(Ordering::Acquire) {
                        break;
                    }
                    let accepted = {
                        let listener = inner.listener.lock().unwrap();
                        let Some(listener) = listener.as_ref() else {
                            break;
                        };
                        listener.accept()
                    };
                    match accepted {
                        Ok((stream, address)) if local_address(address.ip()) => {
                            let service = Self { inner };
                            let Ok(mut stream) = service.socket(stream) else {
                                continue;
                            };
                            if let Err(error) = service.handle(&mut stream, address.ip()) {
                                let _ = write_json(
                                    &mut stream,
                                    &Reply {
                                        ok: false,
                                        error: Some(format!("{error:#}")),
                                        ..Reply::ok()
                                    },
                                );
                                service.inner.status.lock().unwrap().error =
                                    Some(format!("{error:#}"));
                            }
                        }
                        Ok(_) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            drop(inner);
                            // Android only listens in explicit foreground sessions. A slower
                            // accept retry also avoids twenty idle CPU wakeups per second.
                            std::thread::park_timeout(Duration::from_millis(
                                if cfg!(target_os = "android") { 500 } else { 50 },
                            ));
                        }
                        Err(_) => break,
                    }
                }
            })?;
        *service.inner.worker.lock().unwrap() = Some(worker.thread().clone());
        Ok(service)
    }
    pub fn stop(&self) {
        self.inner.stop.store(true, Ordering::Release);
        // Drop the listening socket now, even if the worker is blocked reading audio.
        self.inner.listener.lock().unwrap().take();
        for (_, stream) in self.inner.sockets.lock().unwrap().drain() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        if let Some(worker) = self.inner.worker.lock().unwrap().as_ref() {
            worker.unpark();
        }
        *self.inner.peer.lock().unwrap() = None;
    }
    fn track_socket(&self, stream: &TcpStream) -> Result<SocketGuard> {
        let mut sockets = self.inner.sockets.lock().unwrap();
        self.active()?;
        let id = self.inner.next_socket.fetch_add(1, Ordering::Relaxed);
        sockets.insert(id, stream.try_clone()?);
        Ok(SocketGuard {
            inner: self.inner.clone(),
            id,
        })
    }
    fn socket(&self, stream: TcpStream) -> Result<CancellableStream> {
        configure(&stream)?;
        stream.set_read_timeout(Some(Duration::from_millis(100)))?;
        stream.set_write_timeout(Some(Duration::from_millis(100)))?;
        let guard = self.track_socket(&stream)?;
        Ok(CancellableStream {
            stream,
            guard,
            read_timeout: Some(Duration::from_secs(30)),
        })
    }
    fn connect(&self, target: &PairingCode) -> Result<CancellableStream> {
        self.active()?;
        let stream = TcpStream::connect_timeout(&target.address, Duration::from_secs(8))
            .context("Устройство недоступно. Проверьте локальную сеть и брандмауэр")?;
        self.socket(stream)
    }
    pub fn code(&self) -> &str {
        &self.inner.code
    }
    pub fn status(&self) -> SyncStatus {
        self.inner.status.lock().unwrap().clone()
    }
    pub fn is_enabled(&self) -> bool {
        !self.inner.stop.load(Ordering::Acquire)
    }
    pub fn is_transferring(&self) -> bool {
        self.inner.transferring.load(Ordering::Acquire)
    }
    pub fn qr_image(&self, path: &Path) -> Result<()> {
        qrcode::QrCode::new(self.code())?
            .render::<image::Luma<u8>>()
            .min_dimensions(256, 256)
            .build()
            .save(path)?;
        Ok(())
    }
    fn own_peer(&self) -> Peer {
        Peer {
            device: self.inner.device.clone(),
            code: self.code().into(),
            name: self.inner.name.clone(),
        }
    }
    fn set_peer(&self, peer: Peer) {
        self.inner.status.lock().unwrap().peer = Some(peer.name.clone());
        *self.inner.peer.lock().unwrap() = Some(peer);
    }
    fn active(&self) -> Result<()> {
        ensure!(
            !self.inner.stop.load(Ordering::Acquire),
            "Синхронизация выключена"
        );
        Ok(())
    }
    fn guard(&self) -> Result<TransferGuard<'_>> {
        self.active()?;
        ensure!(
            self.inner
                .transferring
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok(),
            "Дождитесь завершения текущей передачи"
        );
        self.inner.status.lock().unwrap().error = None;
        Ok(TransferGuard(&self.inner.transferring))
    }
    pub fn pair(&self, code: &str) -> Result<()> {
        self.active()?;
        let target = PairingCode::parse(code)?;
        ensure!(
            target.token != self.inner.token,
            "Это код текущего устройства"
        );
        let mut stream = self.connect(&target)?;
        write_json(
            &mut stream,
            &Request::Pair {
                version: VERSION,
                token: target.token,
                peer: self.own_peer(),
            },
        )?;
        let reply = read_json::<Reply>(&mut stream)?.check()?;
        let peer = reply
            .peer
            .context("Устройство не вернуло код подключения")?;
        ensure!(
            Uuid::parse_str(&peer.device).is_ok(),
            "Некорректный ID устройства"
        );
        let returned = PairingCode::parse(&peer.code)?;
        ensure!(
            returned.address == target.address && returned.token == PairingCode::parse(code)?.token,
            "Устройство вернуло другой код подключения"
        );
        self.active()?;
        self.set_peer(peer);
        Ok(())
    }
    fn handle(&self, stream: &mut CancellableStream, address: IpAddr) -> Result<()> {
        let request: Request = read_json(stream)?;
        self.active()?;
        match request {
            Request::Pair {
                version,
                token,
                peer,
            } => {
                ensure!(
                    version == VERSION && token.as_str() == self.inner.token,
                    "Код подключения устарел или неверен"
                );
                ensure!(
                    !self.is_transferring(),
                    "Дождитесь завершения текущей передачи"
                );
                let pairing = PairingCode::parse(&peer.code)?;
                ensure!(
                    pairing.address.ip() == address
                        && pairing.token != self.inner.token
                        && Uuid::parse_str(&peer.device).is_ok()
                        && peer.name.chars().count() <= 100,
                    "Некорректный адрес устройства"
                );
                write_json(
                    stream,
                    &Reply {
                        peer: Some(self.own_peer()),
                        ..Reply::ok()
                    },
                )?;
                self.set_peer(peer);
                Ok(())
            }
            request @ (Request::Transfer { .. } | Request::Inspect { .. }) => {
                let token = match &request {
                    Request::Transfer { token, .. } | Request::Inspect { token, .. } => token,
                    _ => unreachable!(),
                };
                ensure!(
                    token.as_str() == self.inner.token,
                    "Код подключения устарел или неверен"
                );
                let peer = self
                    .inner
                    .peer
                    .lock()
                    .unwrap()
                    .clone()
                    .context("Сначала подключите устройства")?;
                ensure!(
                    PairingCode::parse(&peer.code)?.address.ip() == address,
                    "Передача от неизвестного устройства"
                );
                let source_device = match &request {
                    Request::Transfer { manifest, .. } | Request::Inspect { manifest, .. } => {
                        &manifest.device
                    }
                    _ => unreachable!(),
                };
                ensure!(
                    source_device == &peer.device,
                    "Передача от неизвестного устройства"
                );
                let _guard = self.guard()?;
                match request {
                    Request::Inspect { manifest, .. } => self.inspect(stream, &manifest),
                    Request::Transfer {
                        manifest, copies, ..
                    } => self.receive(stream, manifest, &copies),
                    _ => unreachable!(),
                }
            }
        }
    }
    fn progress(&self, title: &str, receiving: bool, bytes: u64, total: u64, complete: bool) {
        self.inner.status.lock().unwrap().progress = Some(TransferProgress {
            title: title.into(),
            receiving,
            bytes,
            total,
            complete,
        });
    }
}

fn local_ip() -> IpAddr {
    // UDP connect selects a route without sending packets or depending on a public service.
    let route = UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("192.0.2.1:80")?;
            socket.local_addr()
        })
        .map(|address| address.ip())
        .ok()
        .filter(|ip| local_address(*ip) && !ip.is_unspecified() && !ip.is_loopback());
    if let Some(ip) = route {
        return ip;
    }
    // VPN routing may select a tunnel with a globally scoped address. Find an
    // actual local interface instead of silently advertising 127.0.0.1.
    let mut candidates: Vec<_> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|interface| match interface.ip() {
            IpAddr::V4(ip) if ip.is_private() || shared_address(ip) => Some(ip),
            _ => None,
        })
        .collect();
    candidates.sort_by_key(|ip| (!ip.is_private(), ip.octets()));
    candidates
        .first()
        .copied()
        .unwrap_or(Ipv4Addr::LOCALHOST)
        .into()
}
fn hash_file(path: &Path, mut active: impl FnMut() -> Result<()>) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        active()?;
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn validate_manifest(manifest: &Manifest) -> Result<()> {
    ensure!(
        Uuid::parse_str(&manifest.transfer_id).is_ok(),
        "Некорректный ID передачи"
    );
    ensure!(
        !manifest.library || (manifest.playlist.is_none() && !manifest.favorites),
        "Некорректная библиотека"
    );
    ensure!(
        manifest.version == VERSION,
        "Версия протокола синхронизации не поддерживается"
    );
    ensure!(
        Uuid::parse_str(&manifest.device).is_ok(),
        "Некорректный ID устройства"
    );
    ensure!(
        manifest.title.chars().count() <= 500
            && !manifest.title.trim().is_empty()
            && manifest.tracks.len() <= 10000,
        "Некорректная коллекция"
    );
    ensure!(
        !manifest.favorites || manifest.playlist.is_some(),
        "Некорректная коллекция избранного"
    );
    ensure!(
        manifest
            .playlist
            .as_ref()
            .is_none_or(|id| !id.is_empty() && id.len() <= 100),
        "Некорректный ID плейлиста"
    );
    let mut total = 0u64;
    for track in &manifest.tracks {
        ensure!(
            !track.source_id.is_empty() && track.source_id.len() <= 100,
            "Некорректный ID трека"
        );
        ensure!(
            track.title.chars().count() <= 500
                && !track.title.trim().is_empty()
                && track.artist.chars().count() <= 500
                && track.source_url.as_ref().is_none_or(|s| s.len() <= 4096),
            "Некорректные метаданные трека"
        );
        ensure!(
            AUDIO_EXTENSIONS.contains(&track.extension.as_str())
                && track.hash.len() == 64
                && track.hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "Некорректный аудиофайл"
        );
        ensure!(
            track.size > 0 && track.size <= MAX_FILE,
            "Аудиофайл слишком большой или пустой"
        );
        total = total
            .checked_add(track.size)
            .context("Слишком большая коллекция")?;
    }
    if total > MAX_TOTAL {
        bail!("Коллекция превышает 32 ГиБ. Передавайте её по частям");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[track_caller]
    fn wait_until(mut ready: impl FnMut() -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !ready() {
            assert!(
                std::time::Instant::now() < deadline,
                "Worker did not reach expected state"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn assert_closed(stream: &mut TcpStream) {
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        match stream.read(&mut [0; 1]) {
            Ok(0) => {}
            Err(error) => assert!(
                !matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ),
                "Socket stayed open: {error}"
            ),
            result => panic!("Expected a closed socket, got {result:?}"),
        }
    }
    #[test]
    fn stop_closes_listener_and_stalled_incoming_socket_without_waiting_for_timeout() {
        let root = tempfile::tempdir().unwrap();
        let receiver = service(Database::open(root.path()).unwrap());
        let address = PairingCode::parse(receiver.code()).unwrap().address;
        let mut stalled = TcpStream::connect(address).unwrap();
        wait_until(|| !receiver.inner.sockets.lock().unwrap().is_empty());
        // The peer has not sent even the frame header; the old implementation waited 30s here.
        let start = std::time::Instant::now();
        receiver.stop();
        assert!(start.elapsed() < Duration::from_millis(250));
        assert!(!receiver.is_enabled());
        assert!(receiver.inner.listener.lock().unwrap().is_none());
        assert!(receiver.inner.sockets.lock().unwrap().is_empty());
        assert!(TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_err());
        assert_closed(&mut stalled);
        receiver.stop();
        let weak = Arc::downgrade(&receiver.inner);
        drop(receiver);
        wait_until(|| weak.upgrade().is_none());
    }
    #[test]
    fn stop_interrupts_outgoing_duplicate_inspection() {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(root.path()).unwrap();
        let path = root.path().join("song.wav");
        wav(&path, 0);
        let track = TrackService::new(db.clone()).import_local(&path).unwrap();
        let sender = service(db);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        sender.set_peer(Peer {
            device: Uuid::new_v4().to_string(),
            code: format!("shard-sync://{address}/{}", Uuid::new_v4()),
            name: "Stalled receiver".into(),
        });
        let (ready, arrived) = std::sync::mpsc::channel();
        let remote = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            configure(&stream).unwrap();
            assert!(matches!(
                read_json::<Request>(&mut stream).unwrap(),
                Request::Inspect { .. }
            ));
            ready.send(()).unwrap();
            assert_closed(&mut stream);
        });
        let sending = sender.clone();
        let (done, finished) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || done.send(sending.send_track(track.id)).unwrap());
        arrived.recv_timeout(Duration::from_secs(15)).unwrap();
        sender.stop();
        assert!(
            finished
                .recv_timeout(Duration::from_millis(500))
                .unwrap()
                .is_err()
        );
        worker.join().unwrap();
        remote.join().unwrap();
        assert!(!sender.is_transferring());
    }
    #[test]
    fn stop_interrupts_incoming_audio_and_preserves_partial_cache() {
        let a_root = tempfile::tempdir().unwrap();
        let b_root = tempfile::tempdir().unwrap();
        let a_db = Database::open(a_root.path()).unwrap();
        let b_db = Database::open(b_root.path()).unwrap();
        let path = a_root.path().join("song.wav");
        wav(&path, 0);
        let audio = TrackService::new(a_db.clone());
        let track = audio.import_local(&path).unwrap();
        let a = service(a_db);
        let b = service(b_db.clone());
        a.pair(b.code()).unwrap();
        let transfer = manifest(&a, Some(42), &[track]);
        let (mut stream, reply) = start_transfer(&b, transfer.clone(), vec![]);
        assert_eq!(reply.missing, vec![0]);
        let entry = &transfer.tracks[0];
        let bytes = fs::read(&path).unwrap();
        stream.write_all(&bytes[..30000]).unwrap();
        let partial = b_db
            .root()
            .join("sync-cache")
            .join(format!("{}.wav.part", entry.hash));
        wait_until(|| fs::metadata(&partial).is_ok_and(|m| m.len() == 30000));
        b.stop();
        assert_closed(&mut stream);
        wait_until(|| !b.is_transferring());
        assert!(
            TrackRepository::new(b_db.clone())
                .list_tracks()
                .unwrap()
                .is_empty()
        );
        let restarted = SyncService::listen(
            b_db.clone(),
            b.inner.device.clone(),
            "Restarted".into(),
            "127.0.0.1:0".parse().unwrap(),
            Ipv4Addr::LOCALHOST.into(),
        )
        .unwrap();
        a.pair(restarted.code()).unwrap();
        let (mut stream, reply) = start_transfer(&restarted, transfer, vec![]);
        assert_eq!(reply.offsets, vec![30000]);
        stream.write_all(&bytes[30000..]).unwrap();
        let report = read_json::<Reply>(&mut stream)
            .unwrap()
            .check()
            .unwrap()
            .report;
        assert_eq!(report.copied, 1);
        assert_eq!(report.resumed_bytes, 30000);
        a.stop();
        restarted.stop();
    }
    fn wav(path: &Path, sample: u8) {
        let data_size = 160_000u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_size.to_le_bytes());
        bytes.extend(vec![sample; data_size as usize]);
        fs::write(path, bytes).unwrap();
    }
    fn service(db: Database) -> SyncService {
        SyncService::listen(
            db,
            Uuid::new_v4().to_string(),
            "Test device".into(),
            "127.0.0.1:0".parse().unwrap(),
            Ipv4Addr::LOCALHOST.into(),
        )
        .unwrap()
    }
    #[test]
    fn pairing_codes_restrict_hosts_and_credentials() {
        let token = Uuid::new_v4();
        assert!(PairingCode::parse(&format!("shard-sync://192.168.1.4:43567/{token}")).is_ok());
        assert!(PairingCode::parse(&format!("shard-sync://100.83.69.1:43567/{token}")).is_ok());
        for code in [
            format!("shard-sync://8.8.8.8:1234/{token}"),
            format!("https://192.168.1.4:1234/{token}"),
            "shard-sync://192.168.1.4:1234/wrong".into(),
            format!("shard-sync://user@192.168.1.4:1234/{token}"),
        ] {
            assert!(PairingCode::parse(&code).is_err());
        }
    }
    #[test]
    fn transfers_playlist_audio_favorites_and_progress_both_directions_without_duplicates() {
        let a_root = tempfile::tempdir().unwrap();
        let b_root = tempfile::tempdir().unwrap();
        let a_db = Database::open(a_root.path()).unwrap();
        let b_db = Database::open(b_root.path()).unwrap();
        let a_tracks = TrackService::new(a_db.clone());
        let source = a_root.path().join("first.wav");
        wav(&source, 0);
        let first = a_tracks.import_local(&source).unwrap();
        let source2 = a_root.path().join("second.wav");
        wav(&source2, 1);
        let second = a_tracks.import_local(&source2).unwrap();
        let lists = PlaylistService::new(a_db.clone());
        let playlist = lists.create_playlist("Тестовый плейлист").unwrap();
        lists.add_track(playlist, second.id).unwrap();
        lists.add_track(playlist, first.id).unwrap();
        FavoritesService::new(a_db.clone())
            .set(second.id, true)
            .unwrap();
        let a = service(a_db.clone());
        let b = service(b_db.clone());
        assert!(a.send_playlist(playlist).is_err());
        assert!(a.pair(a.code()).is_err());
        a.pair(b.code()).unwrap();
        assert!(a.status().peer.is_some());
        assert!(b.status().peer.is_some());
        a.send_playlist(playlist).unwrap();
        for status in [a.status(), b.status()] {
            let progress = status.progress.unwrap();
            assert!(progress.complete);
            assert!(progress.total > 0);
            assert_eq!(progress.bytes, progress.total);
        }
        let b_lists = PlaylistService::new(b_db.clone());
        let received = b_lists.list_playlists().unwrap().remove(0);
        assert_eq!(received.name, "Тестовый плейлист");
        let actual = b_lists.list_playlist_tracks(received.id).unwrap();
        assert_eq!(
            actual.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
            [second.title.as_str(), first.title.as_str()]
        );
        let b_tracks = TrackService::new(b_db.clone());
        assert_eq!(
            fs::read(b_tracks.audio_path(&actual[0]).unwrap()).unwrap(),
            fs::read(&source2).unwrap()
        );
        assert!(
            FavoritesService::new(b_db.clone())
                .ids()
                .unwrap()
                .contains(&actual[0].id)
        );
        a.send_playlist(playlist).unwrap();
        assert_eq!(b_tracks.list_tracks().unwrap().len(), 2);
        assert_eq!(b_lists.list_playlists().unwrap().len(), 1);
        assert_eq!(a.status().progress.unwrap().total, 0);
        a.send_playlist(FAVORITES_ID).unwrap();
        assert_eq!(b_lists.list_playlists().unwrap().len(), 1);
        // A standalone track travels back without creating a playlist.
        b.send_track(actual[0].id).unwrap();
        assert!(a.status().progress.unwrap().receiving);
        assert_eq!(
            FavoritesService::new(a_db.clone()).tracks().unwrap().len(),
            1
        );
        let qr = a_root.path().join("qr.png");
        a.qr_image(&qr).unwrap();
        assert!(image::open(qr).unwrap().width() >= 256);
        let old_code = b.code().to_owned();
        b.stop();
        assert!(b.send_track(actual[0].id).is_err());
        let restarted = service(b_db);
        assert_ne!(old_code, restarted.code());
        assert!(restarted.status().peer.is_none());
        a.stop();
        restarted.stop();
    }
    #[test]
    fn repeated_identical_audio_in_one_playlist_uses_one_local_file() {
        let a_root = tempfile::tempdir().unwrap();
        let b_root = tempfile::tempdir().unwrap();
        let a_db = Database::open(a_root.path()).unwrap();
        let b_db = Database::open(b_root.path()).unwrap();
        let source = a_root.path().join("same.wav");
        wav(&source, 0);
        let tracks = TrackService::new(a_db.clone());
        let first = tracks.import_local(&source).unwrap();
        let second = tracks.import_local(&source).unwrap();
        let lists = PlaylistService::new(a_db.clone());
        let playlist = lists.create_playlist("Duplicates").unwrap();
        lists.add_track(playlist, first.id).unwrap();
        lists.add_track(playlist, second.id).unwrap();
        let a = service(a_db);
        let b = service(b_db.clone());
        a.pair(b.code()).unwrap();
        a.send_playlist(playlist).unwrap();
        assert_eq!(TrackService::new(b_db).list_tracks().unwrap().len(), 1);
        a.stop();
        b.stop();
    }
    #[test]
    fn corrupt_or_truncated_transfers_do_not_modify_library() {
        let a_root = tempfile::tempdir().unwrap();
        let b_root = tempfile::tempdir().unwrap();
        let a = service(Database::open(a_root.path()).unwrap());
        let db = Database::open(b_root.path()).unwrap();
        let b = service(db.clone());
        a.pair(b.code()).unwrap();
        for corrupt in [false, true] {
            let code = PairingCode::parse(b.code()).unwrap();
            let mut stream = TcpStream::connect(code.address).unwrap();
            configure(&stream).unwrap();
            let manifest = Manifest {
                transfer_id: Uuid::new_v4().to_string(),
                library: false,
                version: VERSION,
                device: a.inner.device.clone(),
                playlist: Some("1".into()),
                title: "Broken".into(),
                favorites: false,
                tracks: vec![WireTrack {
                    source_id: "1".into(),
                    title: "Broken".into(),
                    artist: "Artist".into(),
                    source_url: None,
                    duration_ms: None,
                    extension: "wav".into(),
                    hash: "a".repeat(64),
                    size: 20,
                    favorite: true,
                }],
            };
            write_json(
                &mut stream,
                &Request::Transfer {
                    token: code.token,
                    manifest,
                    copies: vec![],
                },
            )
            .unwrap();
            read_json::<Reply>(&mut stream).unwrap().check().unwrap();
            stream
                .write_all(if corrupt { &[0; 20] } else { &[0; 5] })
                .unwrap();
            stream.shutdown(std::net::Shutdown::Write).unwrap();
            // Unix may reset an aborted frame before a structured error reply
            // reaches the sender. Both are rejection; a timeout is not.
            match read_json::<Reply>(&mut stream) {
                Ok(reply) => assert!(reply.check().is_err()),
                Err(error) => assert!(
                    error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|io| matches!(
                            io.kind(),
                            std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::ConnectionAborted
                                | std::io::ErrorKind::UnexpectedEof
                                | std::io::ErrorKind::BrokenPipe
                        )),
                    "Unexpected transfer failure: {error:#}"
                ),
            }
            assert!(
                TrackService::new(db.clone())
                    .list_tracks()
                    .unwrap()
                    .is_empty()
            );
            assert!(
                PlaylistService::new(db.clone())
                    .list_playlists()
                    .unwrap()
                    .is_empty()
            );
            assert!(FavoritesService::new(db.clone()).ids().unwrap().is_empty());
            assert_eq!(fs::read_dir(db.root().join("audio")).unwrap().count(), 0);
        }
        a.stop();
        b.stop();
    }

    fn manifest(service: &SyncService, playlist: Option<i64>, tracks: &[Track]) -> Manifest {
        let audio = TrackService::new(service.inner.db.clone());
        Manifest {
            transfer_id: Uuid::new_v4().to_string(),
            version: VERSION,
            device: service.inner.device.clone(),
            playlist: playlist.map(|id| id.to_string()),
            library: false,
            title: "Resume test".into(),
            favorites: false,
            tracks: tracks
                .iter()
                .map(|track| {
                    let path = audio.audio_path(track).unwrap();
                    WireTrack {
                        source_id: track.id.to_string(),
                        title: track.title.clone(),
                        artist: track.artist.clone(),
                        source_url: track.source_url.clone(),
                        duration_ms: track.duration_ms,
                        extension: "wav".into(),
                        hash: hash_file(&path, || Ok(())).unwrap(),
                        size: fs::metadata(path).unwrap().len(),
                        favorite: false,
                    }
                })
                .collect(),
        }
    }
    fn start_transfer(
        target: &SyncService,
        manifest: Manifest,
        copies: Vec<usize>,
    ) -> (TcpStream, Reply) {
        let code = PairingCode::parse(target.code()).unwrap();
        let mut stream = TcpStream::connect(code.address).unwrap();
        configure(&stream).unwrap();
        write_json(
            &mut stream,
            &Request::Transfer {
                token: code.token,
                manifest,
                copies,
            },
        )
        .unwrap();
        let reply = read_json::<Reply>(&mut stream).unwrap().check().unwrap();
        (stream, reply)
    }
    #[test]
    fn resumes_completed_and_partial_files_after_receiver_restart_and_replays_ack_without_copies() {
        let a_root = tempfile::tempdir().unwrap();
        let b_root = tempfile::tempdir().unwrap();
        let a_db = Database::open(a_root.path()).unwrap();
        let b_db = Database::open(b_root.path()).unwrap();
        let audio = TrackService::new(a_db.clone());
        let mut tracks = Vec::new();
        for sample in [0, 1] {
            let path = a_root.path().join(format!("{sample}.wav"));
            wav(&path, sample);
            tracks.push(audio.import_local(&path).unwrap());
        }
        let a = service(a_db.clone());
        let b = service(b_db.clone());
        a.pair(b.code()).unwrap();
        let transfer = manifest(&a, Some(99), &tracks);
        let first = fs::read(audio.audio_path(&tracks[0]).unwrap()).unwrap();
        let second = fs::read(audio.audio_path(&tracks[1]).unwrap()).unwrap();
        let (mut stream, reply) = start_transfer(&b, transfer.clone(), vec![]);
        assert_eq!(reply.missing, vec![0, 1]);
        assert_eq!(reply.offsets, vec![0, 0]);
        stream.write_all(&first).unwrap();
        stream.write_all(&second[..50000]).unwrap();
        stream.shutdown(std::net::Shutdown::Write).unwrap();
        assert!(read_json::<Reply>(&mut stream).unwrap().check().is_err());
        assert!(
            TrackService::new(b_db.clone())
                .list_tracks()
                .unwrap()
                .is_empty()
        );
        b.stop();
        let restarted = SyncService::listen(
            Database::open(b_root.path()).unwrap(),
            b.inner.device.clone(),
            "Restarted".into(),
            "127.0.0.1:0".parse().unwrap(),
            Ipv4Addr::LOCALHOST.into(),
        )
        .unwrap();
        a.pair(restarted.code()).unwrap();
        let (mut stream, reply) = start_transfer(&restarted, transfer.clone(), vec![]);
        assert_eq!(reply.missing, vec![1]);
        assert_eq!(reply.offsets, vec![50000]);
        stream.write_all(&second[50000..]).unwrap();
        let report = read_json::<Reply>(&mut stream)
            .unwrap()
            .check()
            .unwrap()
            .report;
        assert_eq!(report.copied, 2);
        assert_eq!(report.resumed_bytes, 50000);
        assert_eq!(
            TrackService::new(b_db.clone()).list_tracks().unwrap().len(),
            2
        );
        // Simulate a sender that lost the final acknowledgement and retries with Copy selected.
        let (mut stream, reply) = start_transfer(&restarted, transfer, vec![0, 1]);
        assert!(reply.done);
        assert!(reply.missing.is_empty());
        read_json::<Reply>(&mut stream).unwrap().check().unwrap();
        assert_eq!(
            TrackService::new(b_db.clone()).list_tracks().unwrap().len(),
            2
        );
        assert_eq!(
            PlaylistService::new(b_db).list_playlists().unwrap().len(),
            1
        );
        a.stop();
        restarted.stop();
    }
    #[test]
    fn local_duplicates_can_be_skipped_copied_or_processed_without_warnings() {
        let a_root = tempfile::tempdir().unwrap();
        let b_root = tempfile::tempdir().unwrap();
        let a_db = Database::open(a_root.path()).unwrap();
        let b_db = Database::open(b_root.path()).unwrap();
        let path = a_root.path().join("song.wav");
        wav(&path, 0);
        let track = TrackService::new(a_db.clone()).import_local(&path).unwrap();
        TrackService::new(b_db.clone()).import_local(&path).unwrap();
        let a = service(a_db);
        let b = service(b_db.clone());
        a.pair(b.code()).unwrap();
        let skipped = a
            .send_selection(
                SyncSelection::Track(track.id),
                SyncOptions::default(),
                |duplicates| {
                    assert_eq!(duplicates.len(), 1);
                    assert_eq!(duplicates[0].kind, DuplicateKind::IdenticalFile);
                    Ok(vec![DuplicateDecision {
                        index: duplicates[0].index,
                        copy: false,
                    }])
                },
            )
            .unwrap();
        assert_eq!(skipped.copied, 0);
        assert_eq!(skipped.reused, 1);
        let copied = a
            .send_selection(
                SyncSelection::Track(track.id),
                SyncOptions::default(),
                |duplicates| {
                    Ok(duplicates
                        .iter()
                        .map(|d| DuplicateDecision {
                            index: d.index,
                            copy: true,
                        })
                        .collect())
                },
            )
            .unwrap();
        assert_eq!(copied.copied, 1);
        a.send_selection(
            SyncSelection::Track(track.id),
            SyncOptions {
                warn_duplicates: false,
                ignore_duplicates: true,
            },
            |_| panic!("Warnings are off"),
        )
        .unwrap();
        assert_eq!(
            TrackService::new(b_db.clone()).list_tracks().unwrap().len(),
            2
        );
        a.send_selection(
            SyncSelection::Track(track.id),
            SyncOptions {
                warn_duplicates: false,
                ignore_duplicates: false,
            },
            |_| panic!("Warnings are off"),
        )
        .unwrap();
        assert_eq!(TrackService::new(b_db).list_tracks().unwrap().len(), 3);
        a.stop();
        b.stop();
    }
    #[test]
    fn metadata_source_matching_and_per_track_decisions_cover_local_imports() {
        let a_root = tempfile::tempdir().unwrap();
        let b_root = tempfile::tempdir().unwrap();
        let a_db = Database::open(a_root.path()).unwrap();
        let b_db = Database::open(b_root.path()).unwrap();
        let a_audio = TrackService::new(a_db.clone());
        let b_audio = TrackService::new(b_db.clone());
        let path = a_root.path().join("a.wav");
        wav(&path, 0);
        let incoming = a_audio.import_local(&path).unwrap();
        a_audio
            .update_metadata(incoming.id, "Song", "Artist")
            .unwrap();
        let path = b_root.path().join("b.wav");
        wav(&path, 1);
        let existing = b_audio.import_local(&path).unwrap();
        b_audio
            .update_metadata(existing.id, "  SONG  ", "artist")
            .unwrap();
        let a = service(a_db.clone());
        let b = service(b_db.clone());
        a.pair(b.code()).unwrap();
        a.send_selection(
            SyncSelection::Track(incoming.id),
            SyncOptions::default(),
            |duplicates| {
                assert_eq!(duplicates[0].kind, DuplicateKind::SimilarMetadata);
                Ok(vec![DuplicateDecision {
                    index: 0,
                    copy: false,
                }])
            },
        )
        .unwrap();
        b_audio
            .update_metadata(existing.id, "Different title", "Other artist")
            .unwrap();
        for db in [&a_db, &b_db] {
            db.connect()
                .unwrap()
                .execute(
                    "UPDATE tracks SET source_url='https://music.yandex.ru/track/123'",
                    [],
                )
                .unwrap();
        }
        a.send_selection(
            SyncSelection::Track(incoming.id),
            SyncOptions::default(),
            |duplicates| {
                assert_eq!(duplicates[0].kind, DuplicateKind::SameSource);
                Ok(vec![DuplicateDecision {
                    index: 0,
                    copy: false,
                }])
            },
        )
        .unwrap();
        assert_eq!(b_audio.list_tracks().unwrap().len(), 1);
        // Two identical entries in one collection can be handled independently.
        let duplicate = a_audio.import_local(&a_root.path().join("a.wav")).unwrap();
        let another_sender = service(a_db.clone());
        another_sender.pair(b.code()).unwrap();
        let lists = PlaylistService::new(a_db);
        let playlist = lists.create_playlist("Choices").unwrap();
        lists.add_track(playlist, incoming.id).unwrap();
        lists.add_track(playlist, duplicate.id).unwrap();
        another_sender
            .send_selection(
                SyncSelection::Playlist(playlist),
                SyncOptions::default(),
                |duplicates| {
                    assert_eq!(duplicates.len(), 2);
                    Ok(duplicates
                        .iter()
                        .map(|d| DuplicateDecision {
                            index: d.index,
                            copy: d.index == 1,
                        })
                        .collect())
                },
            )
            .unwrap();
        assert_eq!(b_audio.list_tracks().unwrap().len(), 2);
        another_sender.stop();
        a.stop();
        b.stop();
    }
    #[test]
    fn new_playlist_detects_metadata_and_source_duplicates_within_the_selection() {
        let a_root = tempfile::tempdir().unwrap();
        let b_root = tempfile::tempdir().unwrap();
        let a_db = Database::open(a_root.path()).unwrap();
        let b_db = Database::open(b_root.path()).unwrap();
        let audio = TrackService::new(a_db.clone());
        let lists = PlaylistService::new(a_db.clone());
        let playlist = lists.create_playlist("Versions").unwrap();
        for sample in 0..3 {
            let path = a_root.path().join(format!("{sample}.wav"));
            wav(&path, sample);
            let track = audio.import_local(&path).unwrap();
            audio
                .update_metadata(
                    track.id,
                    if sample == 2 { "Other title" } else { "Song" },
                    "Artist",
                )
                .unwrap();
            if sample != 1 {
                a_db.connect()
                    .unwrap()
                    .execute(
                        "UPDATE tracks SET source_url='https://example.com/song' WHERE id=?1",
                        [track.id],
                    )
                    .unwrap();
            }
            lists.add_track(playlist, track.id).unwrap();
        }
        let a = service(a_db);
        let b = service(b_db.clone());
        a.pair(b.code()).unwrap();
        let report = a
            .send_selection(
                SyncSelection::Playlist(playlist),
                SyncOptions::default(),
                |duplicates| {
                    assert_eq!(duplicates.len(), 2);
                    assert_eq!(duplicates[0].index, 1);
                    assert_eq!(duplicates[0].kind, DuplicateKind::SimilarMetadata);
                    assert_eq!(duplicates[1].index, 2);
                    assert_eq!(duplicates[1].kind, DuplicateKind::SameSource);
                    Ok(vec![
                        DuplicateDecision {
                            index: 1,
                            copy: false,
                        },
                        DuplicateDecision {
                            index: 2,
                            copy: true,
                        },
                    ])
                },
            )
            .unwrap();
        assert_eq!(report.copied, 2);
        assert_eq!(report.reused, 1);
        let repeated = a
            .send_selection(
                SyncSelection::Playlist(playlist),
                SyncOptions::default(),
                |_| panic!("Previously synchronized origins must be reused"),
            )
            .unwrap();
        assert_eq!(repeated.copied, 0);
        assert_eq!(repeated.reused, 3);
        assert_eq!(
            TrackRepository::new(b_db.clone())
                .list_tracks()
                .unwrap()
                .len(),
            2
        );
        let received = PlaylistService::new(b_db);
        let id = received.list_playlists().unwrap()[0].id;
        assert_eq!(received.list_playlist_tracks(id).unwrap().len(), 2);
        a.stop();
        b.stop();
    }
    #[test]
    fn all_tracks_and_growing_playlists_reuse_previous_transfers_even_with_copy_default() {
        let a_root = tempfile::tempdir().unwrap();
        let b_root = tempfile::tempdir().unwrap();
        let a_db = Database::open(a_root.path()).unwrap();
        let b_db = Database::open(b_root.path()).unwrap();
        let audio = TrackService::new(a_db.clone());
        let lists = PlaylistService::new(a_db.clone());
        let playlist = lists.create_playlist("Growing").unwrap();
        let a = service(a_db);
        let b = service(b_db.clone());
        a.pair(b.code()).unwrap();
        let mut ids = Vec::new();
        for sample in [0, 1, 2] {
            let path = a_root.path().join(format!("{sample}.wav"));
            wav(&path, sample);
            let track = audio.import_local(&path).unwrap();
            ids.push(track.id);
            lists.add_track(playlist, track.id).unwrap();
            let report = a
                .send_selection(
                    SyncSelection::Playlist(playlist),
                    SyncOptions::default(),
                    |_| panic!("Known synchronized tracks are incremental, not warnings"),
                )
                .unwrap();
            assert_eq!(report.copied, 1);
            assert_eq!(report.reused, sample as usize);
        }
        let report = a
            .send_selection(SyncSelection::Library, SyncOptions::default(), |_| {
                panic!("Known tracks should be reused across scopes")
            })
            .unwrap();
        assert_eq!(report.copied, 0);
        assert_eq!(report.reused, 3);
        assert_eq!(
            TrackService::new(b_db.clone()).list_tracks().unwrap().len(),
            3
        );
        assert_eq!(
            PlaylistService::new(b_db.clone())
                .list_playlists()
                .unwrap()
                .len(),
            1
        );
        let path = a_root.path().join("outside.wav");
        wav(&path, 3);
        audio.import_local(&path).unwrap();
        a.send_selection(SyncSelection::Library, SyncOptions::default(), |_| {
            panic!("No new duplicates")
        })
        .unwrap();
        assert_eq!(
            TrackService::new(b_db.clone()).list_tracks().unwrap().len(),
            4
        );
        assert_eq!(
            PlaylistService::new(b_db).list_playlists().unwrap().len(),
            1
        );
        a.stop();
        b.stop();
    }
    #[test]
    fn database_failure_retains_audio_for_retry_and_leaves_no_orphan_library_files() {
        let a_root = tempfile::tempdir().unwrap();
        let b_root = tempfile::tempdir().unwrap();
        let a_db = Database::open(a_root.path()).unwrap();
        let b_db = Database::open(b_root.path()).unwrap();
        let path = a_root.path().join("song.wav");
        wav(&path, 0);
        let track = TrackService::new(a_db.clone()).import_local(&path).unwrap();
        b_db.connect().unwrap().execute_batch("CREATE TRIGGER reject_sync BEFORE INSERT ON tracks BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
        let a = service(a_db);
        let b = service(b_db.clone());
        a.pair(b.code()).unwrap();
        assert!(a.send_track(track.id).is_err());
        assert_eq!(fs::read_dir(b_db.root().join("audio")).unwrap().count(), 0);
        assert!(
            TrackService::new(b_db.clone())
                .list_tracks()
                .unwrap()
                .is_empty()
        );
        b_db.connect()
            .unwrap()
            .execute_batch("DROP TRIGGER reject_sync;")
            .unwrap();
        a.send_track(track.id).unwrap();
        assert_eq!(a.status().progress.unwrap().total, 0);
        assert_eq!(TrackService::new(b_db).list_tracks().unwrap().len(), 1);
        a.stop();
        b.stop();
    }
    #[test]
    fn cancelled_duplicate_review_copies_nothing_and_does_not_block_future_transfers() {
        let a_root = tempfile::tempdir().unwrap();
        let b_root = tempfile::tempdir().unwrap();
        let a_db = Database::open(a_root.path()).unwrap();
        let b_db = Database::open(b_root.path()).unwrap();
        let path = a_root.path().join("song.wav");
        wav(&path, 0);
        let track = TrackService::new(a_db.clone()).import_local(&path).unwrap();
        TrackService::new(b_db.clone()).import_local(&path).unwrap();
        let a = service(a_db);
        let b = service(b_db.clone());
        a.pair(b.code()).unwrap();
        assert!(
            a.send_selection(
                SyncSelection::Track(track.id),
                SyncOptions::default(),
                |_| bail!("Cancelled")
            )
            .is_err()
        );
        assert!(!a.is_transferring());
        assert!(!b.is_transferring());
        assert_eq!(TrackService::new(b_db).list_tracks().unwrap().len(), 1);
        a.send_track(track.id).unwrap();
        a.stop();
        b.stop();
    }
}
