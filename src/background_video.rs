//! Bounded, silent background decoding. FFmpeg never opens an audio device.
use crate::{download_tools, process_tree::DownloadProcess};
use anyhow::{Context, Result, ensure};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};

pub fn is_animated(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "gif" | "mp4" | "webm" | "mkv" | "mov" | "avi" | "m4v"
            )
        })
}

fn command(root: &Path) -> Result<Command> {
    let mut command = Command::new(download_tools::ffmpeg(root)?);
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-threads",
        "2",
        "-filter_threads",
        "2",
    ]);
    command
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped());
    Ok(command)
}

pub fn poster(root: &Path, source: &Path) -> Result<image::DynamicImage> {
    // Bound both the decoded dimensions and bytes read from an untrusted media file.
    let mut command = command(root)?;
    command.arg("-i").arg(source);
    command.args([
        "-map",
        "0:v:0",
        "-an",
        "-sn",
        "-dn",
        "-vf",
        "scale=960:960:force_original_aspect_ratio=decrease",
        "-frames:v",
        "1",
        "-f",
        "image2pipe",
        "-vcodec",
        "png",
        "pipe:1",
    ]);
    let mut process = DownloadProcess::spawn(&mut command)?;
    let output = process
        .child
        .stdout
        .take()
        .context("Missing video output")?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("shard-background-poster".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let result = output
                .take(8 * 1024 * 1024)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = sender.send(result);
        })?;
    let bytes = receiver
        .recv_timeout(std::time::Duration::from_secs(20))
        .context("Background preview timed out")??;
    ensure!(
        process.child.wait()?.success(),
        crate::localized_format!(
            "Не удалось открыть GIF или видео",
            "Could not open GIF or video"
        )
    );
    Ok(image::load_from_memory(&bytes)?)
}

pub struct BackgroundVideo {
    process: Arc<Mutex<DownloadProcess>>,
    pub frames: async_channel::Receiver<Vec<u8>>,
}

impl BackgroundVideo {
    /// Wait for the decoder to release its file before replacing/removing it.
    /// Call only from a background worker.
    pub fn stop(&self) {
        self.frames.close();
        self.process.lock().unwrap().stop();
    }
    pub fn start(root: &Path, source: &Path, width: u32, height: u32, blur: u8) -> Result<Self> {
        ensure!(
            width > 0 && height > 0 && u64::from(width) * u64::from(height) <= 960 * 960,
            "Invalid background dimensions"
        );
        let mut command = command(root)?;
        // Input pacing also applies across loop boundaries. No audio is decoded or emitted.
        command
            .args(["-re", "-stream_loop", "-1", "-i"])
            .arg(source);
        let mut filter = format!("fps=24,scale={width}:{height}");
        if blur > 0 {
            filter.push_str(&format!(",gblur=sigma={blur}"));
        }
        command
            .args([
                "-map", "0:v:0", "-an", "-sn", "-dn", "-vf", &filter, "-pix_fmt", "bgra", "-f",
                "rawvideo", "pipe:1",
            ])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::piped());
        let mut process = DownloadProcess::spawn(&mut command)?;
        let mut output = process
            .child
            .stdout
            .take()
            .context("Missing video output")?;
        let (sender, frames) = async_channel::bounded(1);
        std::thread::Builder::new()
            .name("shard-background-video".into())
            .spawn(move || {
                loop {
                    let mut frame = vec![0; width as usize * height as usize * 4];
                    if output.read_exact(&mut frame).is_err()
                        || sender.send_blocking(frame).is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            process: Arc::new(Mutex::new(process)),
            frames,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backgrounds::BackgroundService, db::Database};
    use std::time::{Duration, Instant};

    fn fixture(root: &Path, extension: &str) -> std::path::PathBuf {
        let source = root.join(format!("animated source.{extension}"));
        let mut command =
            Command::new(download_tools::ffmpeg(root).expect("Install FFmpeg for media tests"));
        command
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=96x64:rate=24",
                "-t",
                "0.5",
                "-y",
            ])
            .arg(&source)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        assert!(
            DownloadProcess::spawn(&mut command)
                .unwrap()
                .child
                .wait()
                .unwrap()
                .success()
        );
        source
    }

    fn frame(video: &BackgroundVideo) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match video.frames.try_recv() {
                Ok(frame) => return frame,
                Err(async_channel::TryRecvError::Empty) => {
                    assert!(Instant::now() < deadline, "Background decoder stalled");
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("Background decoder stopped: {error}"),
            }
        }
    }

    #[test]
    fn gif_and_video_loop_blur_persist_and_clean_up() {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(root.path()).unwrap();
        let service = BackgroundService::new(db.clone());
        for extension in ["gif", "mp4"] {
            let source = fixture(root.path(), extension);
            service.set_media(0, &source).unwrap();
            let background = service.list().unwrap().remove(0);
            assert!(background.is_animated());
            assert_eq!(
                std::fs::read(&source).unwrap(),
                std::fs::read(root.path().join(&background.original_path)).unwrap()
            );
            service.set_blur(0, 0).unwrap();
            let clear = service.list().unwrap().remove(0);
            let clear_image = image::open(root.path().join(&clear.display_path)).unwrap();
            let (width, height) = (clear_image.width(), clear_image.height());
            let video = BackgroundVideo::start(
                root.path(),
                &root.path().join(&clear.original_path),
                width,
                height,
                0,
            )
            .unwrap();
            let first = frame(&video);
            assert_eq!(first.len(), width as usize * height as usize * 4);
            let mut changed = false;
            // The source has 12 frames; consume enough to pass multiple loop boundaries.
            for _ in 0..32 {
                changed |= frame(&video) != first;
            }
            assert!(changed, "Animation froze on its first frame");
            assert!(video.frames.len() <= 1, "Frame queue grew without bound");
            video.stop();
            service.set_blur(0, 20).unwrap();
            let blurred = service.list().unwrap().remove(0);
            assert_eq!(blurred.blur, 20);
            assert_eq!(blurred.original_path, clear.original_path);
            assert_ne!(
                clear_image.to_rgba8(),
                image::open(root.path().join(&blurred.display_path))
                    .unwrap()
                    .to_rgba8()
            );
            let video = BackgroundVideo::start(
                root.path(),
                &root.path().join(&blurred.original_path),
                width,
                height,
                20,
            )
            .unwrap();
            assert_ne!(frame(&video), first, "Video blur was not applied");
            video.stop();
            let bad = root.path().join("bad.mp4");
            std::fs::write(&bad, b"not a video").unwrap();
            assert!(service.set_media(0, &bad).is_err());
            assert_eq!(service.list().unwrap(), vec![blurred.clone()]);
            assert_eq!(
                BackgroundService::new(Database::open(root.path()).unwrap())
                    .list()
                    .unwrap(),
                vec![blurred.clone()]
            );
            service.remove(0).unwrap();
            assert!(!root.path().join(&blurred.original_path).exists());
            assert!(!root.path().join(&blurred.display_path).exists());
            assert!(source.exists());
        }
    }
}

impl Drop for BackgroundVideo {
    fn drop(&mut self) {
        self.frames.close();
        // Termination is nonblocking for the GPUI thread, including a decoder waiting on IO.
        let process = self.process.clone();
        let _ = std::thread::Builder::new()
            .name("shard-background-stop".into())
            .spawn(move || {
                process.lock().unwrap().stop();
            });
    }
}
