use super::*;
use shard::pets::CursorPet;

const WIDTH: f32 = 48.;
const HEIGHT: f32 = 36.;

#[derive(Default)]
struct Motion {
    x: f32,
    y: f32,
    target_x: f32,
    target_y: f32,
    width: f32,
    height: f32,
    moving: bool,
    direction: usize,
}
impl Motion {
    fn bounds(&mut self, width: f32, height: f32) {
        self.width = (width - WIDTH).max(0.);
        self.height = (height - HEIGHT).max(0.);
        self.x = self.x.clamp(0., self.width);
        self.y = self.y.clamp(0., self.height);
        self.target_x = self.target_x.clamp(0., self.width);
        self.target_y = self.target_y.clamp(0., self.height);
    }
    fn follow(&mut self, x: f32, y: f32) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        // Sit beside the pointer, leaving its hotspot and the clicked control free.
        self.target_x = (x + 16.).clamp(0., self.width);
        self.target_y = (y + 14.).clamp(0., self.height);
        self.moving = (self.target_x - self.x).hypot(self.target_y - self.y) > 0.5;
    }
    fn step(&mut self, dt: f32, speed: f32) -> bool {
        let dx = self.target_x - self.x;
        let dy = self.target_y - self.y;
        let distance = dx.hypot(dy);
        if distance <= 0.5 {
            self.moving = false;
            return false;
        }
        let step = ((distance * 5.).clamp(80., 520.) * speed * dt.clamp(0., 0.1)).min(distance);
        // Hysteresis at diagonal boundaries prevents rapid side/front flicker.
        let vertical = if self.direction >= 2 {
            dy.abs() * 1.2 >= dx.abs()
        } else {
            dy.abs() > dx.abs() * 1.2
        };
        self.direction = if vertical {
            if dy > 0. { 2 } else { 3 }
        } else {
            usize::from(dx < 0.)
        };
        self.x += dx / distance * step;
        self.y += dy / distance * step;
        self.moving = distance - step > 0.5;
        true
    }
}

/// An independent, hit-through overlay. Movement follows display frames;
/// standing pets schedule no callbacks and never invalidate the library.
pub(super) struct CursorCompanion {
    kind: CursorPet,
    frames: [Vec<Arc<RenderImage>>; 4],
    motion: Motion,
    frame: usize,
    speed: f32,
    animation_elapsed: f32,
    ticks: u64,
    frame_pending: bool,
    last_frame: Option<Instant>,
}
impl CursorCompanion {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.on_release(|view, cx| {
            for frames in &mut view.frames {
                for image in frames.drain(..) {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();
        Self {
            kind: CursorPet::None,
            frames: std::array::from_fn(|_| vec![]),
            motion: Motion::default(),
            frame: 0,
            speed: 0.5,
            animation_elapsed: 0.,
            ticks: 0,
            frame_pending: false,
            last_frame: None,
        }
    }

    fn schedule_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.frame_pending || !self.motion.moving {
            return;
        }
        self.frame_pending = true;
        cx.on_next_frame(window, |view, window, cx| {
            view.frame_pending = false;
            if view.kind == CursorPet::None || !window.is_window_active() {
                view.motion.moving = false;
                view.last_frame = None;
                return;
            }
            let now = Instant::now();
            let elapsed = view
                .last_frame
                .replace(now)
                .map_or(0., |last| now.duration_since(last).as_secs_f32());
            if view.motion.step(elapsed, view.speed) {
                view.ticks += 1;
                view.animation_elapsed += elapsed;
                view.frame = if view.motion.moving {
                    (view.animation_elapsed / 0.08) as usize % 8
                } else {
                    view.animation_elapsed = 0.;
                    view.last_frame = None;
                    0
                };
                cx.notify();
            } else {
                view.last_frame = None;
                view.animation_elapsed = 0.;
                if view.frame != 0 {
                    view.frame = 0;
                    cx.notify();
                }
            }
            view.schedule_frame(window, cx);
        });
    }

    pub fn configure(
        &mut self,
        kind: CursorPet,
        speed: u16,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Updating speed preserves position, direction and the current animation phase.
        self.speed = f32::from(speed.clamp(20, 150)) / 100.;
        let size = window.viewport_size();
        let old = (self.motion.x, self.motion.y);
        self.motion
            .bounds(size.width / px(1.), size.height / px(1.));
        if kind != self.kind {
            for frames in &mut self.frames {
                for image in frames.drain(..) {
                    cx.drop_image(image, Some(window));
                }
            }
            self.frames = companion_frames(kind);
            self.kind = kind;
            self.frame = 0;
            self.animation_elapsed = 0.;
            self.last_frame = None;
            self.motion.moving = false;
            self.motion.x = (self.motion.width - 16.).max(0.);
            self.motion.y = (self.motion.height - 16.).max(0.);
            if kind != CursorPet::None {
                self.follow(window.mouse_position(), window, cx);
            }
            cx.notify();
        } else if old != (self.motion.x, self.motion.y) {
            cx.notify();
        }
    }
    pub fn follow(&mut self, target: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.kind != CursorPet::None {
            self.motion.follow(target.x / px(1.), target.y / px(1.));
            if self.motion.moving {
                self.schedule_frame(window, cx);
            }
        }
    }
    #[allow(dead_code)]
    pub(super) fn diagnostics(&self) -> (bool, u64, usize) {
        let bytes = self
            .frames
            .iter()
            .flatten()
            .map(|frame| frame.as_bytes(0).map_or(0, <[u8]>::len))
            .sum();
        (self.motion.moving, self.ticks, bytes)
    }
}

fn companion_frames(kind: CursorPet) -> [Vec<Arc<RenderImage>>; 4] {
    let Some(bytes) = kind.asset() else {
        return std::array::from_fn(|_| vec![]);
    };
    let Ok(atlas) = image::load_from_memory(bytes) else {
        return std::array::from_fn(|_| vec![]);
    };
    let atlas = atlas.to_rgba8();
    std::array::from_fn(|direction| {
        (0..8)
            .map(|i| {
                let tile = image::imageops::crop_imm(&atlas, i * 32, direction as u32 * 24, 32, 24)
                    .to_image();
                let mut tile = image::imageops::resize(
                    &tile,
                    WIDTH as u32,
                    HEIGHT as u32,
                    image::imageops::FilterType::Nearest,
                );
                for pixel in tile.pixels_mut() {
                    pixel.0.swap(0, 2);
                }
                Arc::new(RenderImage::new(vec![image::Frame::new(tile)]))
            })
            .collect()
    })
}

impl Render for CursorCompanion {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .inset_0()
            .children(
                self.frames[self.motion.direction]
                    .get(self.frame)
                    .map(|frame| {
                        div()
                            .absolute()
                            .left(px(self.motion.x))
                            .top(px(self.motion.y))
                            .w(px(WIDTH))
                            .h(px(HEIGHT))
                            .child(img(frame.clone()).w(px(WIDTH)).h(px(HEIGHT)))
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{Motion, companion_frames};
    use shard::pets::CursorPet;
    #[test]
    fn vertical_views_and_diagonal_hysteresis_follow_actual_motion() {
        let mut motion = Motion::default();
        motion.bounds(1000., 1000.);
        motion.x = 400.;
        motion.y = 400.;
        for (x, y, direction) in [
            (600., 400., 0),
            (100., 400., 1),
            (400., 600., 2),
            (400., 100., 3),
        ] {
            motion.x = 400.;
            motion.y = 400.;
            motion.follow(x - 16., y - 14.);
            motion.step(1. / 144., 0.5);
            assert_eq!(motion.direction, direction);
        }
        motion.x = 400.;
        motion.y = 400.;
        motion.direction = 0;
        motion.follow(584., 591.);
        motion.step(1. / 144., 0.5);
        assert_eq!(
            motion.direction, 0,
            "A near diagonal must not flicker between views"
        );
    }
    #[test]
    fn speed_changes_travel_without_teleporting_and_extremes_settle() {
        let mut slow = Motion::default();
        let mut fast = Motion::default();
        for motion in [&mut slow, &mut fast] {
            motion.bounds(760., 520.);
            motion.follow(500., 300.);
        }
        slow.step(0.04, 0.2);
        fast.step(0.04, 1.5);
        assert!((fast.x / slow.x - 7.5).abs() < 0.001);
        let previous = (slow.x, slow.y);
        slow.step(0.04, 1.5);
        assert!((slow.x - previous.0).hypot(slow.y - previous.1) <= 31.21);
        for (motion, speed) in [(&mut slow, 0.2), (&mut fast, 1.5)] {
            for _ in 0..1000 {
                motion.step(0.04, speed);
            }
            assert!(!motion.moving);
            assert!((motion.x - 516.).hypot(motion.y - 314.) < 0.51);
        }
    }
    #[test]
    fn companion_follows_settles_clamps_resizes_and_ignores_invalid_pointer() {
        let mut motion = Motion::default();
        motion.bounds(760., 520.);
        motion.follow(400., 250.);
        for _ in 0..300 {
            motion.step(0.04, 0.5);
        }
        assert!(!motion.moving);
        assert!((motion.x - 416.).abs() < 1. && (motion.y - 264.).abs() < 1.);
        let previous = (motion.x, motion.y);
        assert!(!motion.step(0.04, 0.5));
        assert_eq!(previous, (motion.x, motion.y));
        motion.follow(-100., -100.);
        motion.step(0.04, 0.5);
        assert_eq!(motion.direction, 1);
        motion.follow(f32::NAN, f32::INFINITY);
        assert!(motion.target_x.is_finite() && motion.target_y.is_finite());
        motion.follow(900., 900.);
        for _ in 0..300 {
            motion.step(0.04, 0.5);
        }
        assert!(motion.x <= 712. && motion.y <= 484.);
        motion.bounds(100., 80.);
        assert!(motion.x <= 52. && motion.y <= 44.);
        motion.bounds(0., 0.);
        assert_eq!((motion.x, motion.y), (0., 0.));
    }
    #[test]
    fn frames_are_bounded_and_four_distinct_directions_are_prepared_once() {
        for kind in [CursorPet::Kitten, CursorPet::Frog, CursorPet::Pig] {
            let frames = companion_frames(kind);
            assert!(frames.iter().all(|direction| direction.len() == 8));
            let bytes: usize = frames
                .iter()
                .flatten()
                .map(|frame| frame.as_bytes(0).unwrap().len())
                .sum();
            assert_eq!(bytes, 216 * 1024);
            assert_ne!(frames[0][0].as_bytes(0), frames[1][0].as_bytes(0));
            assert_ne!(frames[2][0].as_bytes(0), frames[3][0].as_bytes(0));
            assert_ne!(frames[0][0].as_bytes(0), frames[2][0].as_bytes(0));
        }
        assert!(companion_frames(CursorPet::None).iter().all(Vec::is_empty));
    }
}
