use super::*;
use shard::background_video::BackgroundVideo;

#[derive(Default)]
pub(super) struct BackgroundAnimation {
    key: Option<Background>,
    video: Option<BackgroundVideo>,
    task: Option<Task<()>>,
    surface: Option<Entity<VideoSurface>>,
    suspended: bool,
}

impl BackgroundAnimation {
    #[allow(dead_code)]
    pub(super) fn displayed_frames(&self, cx: &App) -> u64 {
        self.surface
            .as_ref()
            .map_or(0, |surface| surface.read(cx).frames)
    }
    pub fn suspend(&mut self, cx: &mut App) -> Option<BackgroundVideo> {
        self.suspended = true;
        self.task = None;
        self.key = None;
        if let Some(surface) = self.surface.take() {
            surface.update(cx, |view, cx| {
                if let Some(image) = view.image.take() {
                    cx.drop_image(image, None);
                }
            });
        }
        self.video.take()
    }

    pub fn resume(&mut self) {
        self.suspended = false;
    }

    pub fn clear(&mut self, window: &mut Window, cx: &mut App) {
        self.task = None;
        self.video = None;
        self.key = None;
        if let Some(surface) = self.surface.take() {
            surface.update(cx, |view, cx| {
                if let Some(image) = view.image.take() {
                    cx.drop_image(image, Some(window));
                }
            });
        }
    }
}

impl MusicApp {
    pub(super) fn update_background_animation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.background_animation.suspended {
            return;
        }
        let background = self
            .active_background()
            .filter(|b| b.is_animated())
            .cloned();
        if background == self.background_animation.key {
            return;
        }
        self.background_animation.clear(window, cx);
        self.background_animation.key = background.clone();
        let Some(background) = background else {
            return;
        };
        let surface = cx.new(|_| VideoSurface {
            image: None,
            frames: 0,
        });
        self.background_animation.surface = Some(surface.clone());
        let root = self.data_directory.clone();
        let key = background.clone();
        let load = cx.background_executor().spawn(async move {
            let (width, height) = image::image_dimensions(root.join(&background.display_path))?;
            let video = BackgroundVideo::start(
                &root,
                &root.join(&background.original_path),
                width,
                height,
                background.blur,
            )?;
            anyhow::Ok((video, width, height))
        });
        self.background_animation.task = Some(cx.spawn_in(window, async move |this, cx| {
            let (video, width, height) = match load.await {
                Ok(video) => video,
                Err(error) => {
                    let _ = this.update(cx, |app, cx| {
                        app.error = Some(format!("{error:#}"));
                        cx.notify();
                    });
                    return;
                }
            };
            let frames = video.frames.clone();
            if this
                .update(cx, |app, _| app.background_animation.video = Some(video))
                .is_err()
            {
                return;
            }
            loop {
                // A hidden/inactive window should not keep decoding and uploading 24 fps.
                match this.update_in(cx, |app, window, _| {
                    window.is_window_active() || app.preferences.refresh_inactive
                }) {
                    Ok(false) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(400))
                            .await;
                        continue;
                    }
                    Err(_) => return,
                    Ok(true) => {}
                }
                let Ok(bytes) = frames.recv().await else {
                    break;
                };
                let Some(buffer) = image::RgbaImage::from_raw(width, height, bytes) else {
                    break;
                };
                // RenderImage consumes BGRA bytes; FFmpeg already supplies that layout.
                let image = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));
                let current = this.update_in(cx, |app, window, cx| {
                    if app.background_animation.key.as_ref() != Some(&key) {
                        return false;
                    }
                    surface.update(cx, |view, cx| {
                        view.frames += 1;
                        if let Some(old) = view.image.replace(image) {
                            cx.drop_image(old, Some(window));
                        }
                        cx.notify();
                    });
                    true
                });
                if !matches!(current, Ok(true)) {
                    return;
                }
            }
            let _ = this.update(cx, |app, cx| {
                if app.background_animation.key.as_ref() == Some(&key) {
                    app.background_animation.video = None;
                    app.error = Some(localized_format!(
                        "Воспроизведение фона прервалось. Выберите фон заново.",
                        "Background playback stopped. Select the background again."
                    ));
                    cx.notify();
                }
            });
        }));
    }

    // Settings preview uses the poster and never mounts a second decoder/view.
    pub(super) fn background_image(&self, background: &Background) -> ImageSource {
        self.data_directory.join(&background.display_path).into()
    }
    pub(super) fn background_visual(&self, background: &Background) -> AnyElement {
        if self.background_animation.key.as_ref() == Some(background)
            && let Some(surface) = &self.background_animation.surface
        {
            return div().size_full().child(surface.clone()).into_any_element();
        }
        img(self.background_image(background))
            .image_cache(&self.images)
            .size_full()
            .object_fit(ObjectFit::Cover)
            .into_any_element()
    }
}

pub(super) struct VideoSurface {
    image: Option<Arc<RenderImage>>,
    frames: u64,
}
impl Render for VideoSurface {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().children(
            self.image
                .as_ref()
                .map(|image| img(image.clone()).size_full().object_fit(ObjectFit::Cover)),
        )
    }
}
