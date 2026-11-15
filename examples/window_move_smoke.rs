//! Native regression: moving a GPUI window must not rebuild its unchanged scene.
//! Run with cargo run --locked --example window_move_smoke.
#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use anyhow::{Context as _, ensure};
    use gpui::{prelude::*, *};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;
    use windows_sys::Win32::{
        Foundation::{HWND, RECT},
        UI::WindowsAndMessaging::*,
    };

    struct Scene {
        renders: Arc<AtomicUsize>,
    }
    impl Render for Scene {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.fetch_add(1, Ordering::Relaxed);
            div()
                .size_full()
                .bg(rgb(0x152015))
                .text_color(rgb(0xffffff))
                .flex()
                .flex_col()
                .overflow_hidden()
                .children((0..100).map(|i| div().child(format!("Track {i} · Исполнитель · 🎵"))))
        }
    }
    let result = Arc::new(Mutex::new(None));
    let outcome = result.clone();
    Application::new().run(move |cx| {
        let renders = Arc::new(AtomicUsize::new(0));
        let bounds_events = Arc::new(AtomicUsize::new(0));
        let viewport_width = Arc::new(AtomicUsize::new(0));
        let stats = (
            renders.clone(),
            bounds_events.clone(),
            viewport_width.clone(),
        );
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(760.), px(520.)),
                        cx,
                    ))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Shard — Window move regression".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    cx.new(|cx| {
                        cx.observe_window_bounds(window, move |_, window, _| {
                            bounds_events.fetch_add(1, Ordering::Relaxed);
                            viewport_width.store(
                                f32::from(window.viewport_size().width) as usize,
                                Ordering::Relaxed,
                            );
                        })
                        .detach();
                        Scene { renders }
                    })
                },
            )
            .unwrap();
        let hwnd = window
            .update(cx, |_, window, _| {
                match window.window_handle().unwrap().as_raw() {
                    RawWindowHandle::Win32(handle) => handle.hwnd.get() as usize,
                    _ => unreachable!(),
                }
            })
            .unwrap();
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(700))
                .await;
            let report = cx
                .background_executor()
                .spawn(async move {
                    let (renders, bounds_events, viewport_width) = stats;
                    let hwnd = hwnd as HWND;
                    let mut rect = RECT::default();
                    ensure!(
                        unsafe { GetWindowRect(hwnd, &mut rect) } != 0,
                        "GetWindowRect failed"
                    );
                    let before = renders.load(Ordering::Relaxed);
                    let events_before = bounds_events.load(Ordering::Relaxed);
                    for i in 0..100 {
                        ensure!(
                            unsafe {
                                SetWindowPos(
                                    hwnd,
                                    std::ptr::null_mut(),
                                    rect.left + (i % 20 - 10) * 3,
                                    rect.top,
                                    0,
                                    0,
                                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                                )
                            } != 0,
                            "Move failed"
                        );
                        std::thread::sleep(Duration::from_millis(16));
                    }
                    std::thread::sleep(Duration::from_millis(150));
                    let move_renders = renders.load(Ordering::Relaxed) - before;
                    let moves = bounds_events.load(Ordering::Relaxed) - events_before;
                    println!(
                        "Native moves: {moves}; scene renders during movement: {move_renders}"
                    );
                    ensure!(moves >= 90, "Bounds observers stopped receiving moves");
                    ensure!(
                        move_renders <= 5,
                        "Moving a fixed-size window rebuilt the scene {move_renders} times"
                    );
                    let before_resize = renders.load(Ordering::Relaxed);
                    let old_width = viewport_width.load(Ordering::Relaxed);
                    ensure!(
                        unsafe {
                            SetWindowPos(
                                hwnd,
                                std::ptr::null_mut(),
                                rect.left,
                                rect.top,
                                rect.right - rect.left + 80,
                                rect.bottom - rect.top,
                                SWP_NOZORDER | SWP_NOACTIVATE,
                            )
                        } != 0,
                        "Resize failed"
                    );
                    std::thread::sleep(Duration::from_millis(300));
                    ensure!(
                        renders.load(Ordering::Relaxed) > before_resize,
                        "Resize did not redraw"
                    );
                    ensure!(
                        viewport_width.load(Ordering::Relaxed) == old_width + 80,
                        "Viewport did not update after resize"
                    );
                    println!("Resize invalidation and viewport update: passed");
                    Ok::<_, anyhow::Error>(())
                })
                .await;
            *outcome.lock().unwrap() = Some(report);
            let _ = cx.update(|cx| cx.quit());
        })
        .detach();
    });
    result
        .lock()
        .unwrap()
        .take()
        .context("Window regression did not complete")?
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This regression requires Windows.");
}
