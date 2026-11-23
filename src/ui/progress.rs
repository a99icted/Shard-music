use super::*;
use shard::pets::{PetColor, ProgressPet};

// A tiny GPU-painted light; no blur buffers, textures or independent timer.
fn ember(seconds: f32) -> Div {
    let pulse = 0.7 + 0.3 * (seconds * std::f32::consts::TAU / 2.4).sin();
    div()
        .absolute()
        .left(px(51.))
        .top(px(28.))
        .size(px(3.))
        .rounded_full()
        .bg(rgb(0xffc07b))
        .opacity(pulse)
        .shadow(vec![BoxShadow {
            color: rgba(0xff8c4090).into(),
            offset: point(px(0.), px(0.)),
            blur_radius: px(5.),
            spread_radius: px(1.5),
        }])
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct ProgressConfig {
    pub duration: u64,
    pub track: Option<i64>,
    pub theme: Theme,
    pub pet: ProgressPet,
    pub color: PetColor,
    pub inactive: bool,
}

pub(super) struct PlaybackProgress {
    player: Player,
    config: ProgressConfig,
    pub(super) frames: Vec<Arc<RenderImage>>,
    pub(super) bounds: Option<Bounds<Pixels>>,
    preview: Option<u64>,
    position: u64,
    phase: u64,
    _timer: Task<()>,
}

impl PlaybackProgress {
    pub fn new(player: Player, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let timer = cx.spawn_in(window, async move |this, cx| {
            loop {
                let delay = this
                    .read_with(cx, |view, _| {
                        if view.config.pet == ProgressPet::None {
                            200
                        } else {
                            80
                        }
                    })
                    .unwrap_or(500);
                cx.background_executor()
                    .timer(Duration::from_millis(delay))
                    .await;
                if this
                    .update_in(cx, |view, window, cx| {
                        if !window.is_window_active() && !view.config.inactive {
                            return;
                        }
                        let (track, _, position) = view.player.progress();
                        if track != view.config.track {
                            return;
                        }
                        let animated = view.config.pet != ProgressPet::None;
                        if animated {
                            view.phase = (view.phase + 1) % 840;
                        }
                        if (position / 100 != view.position / 100) || animated {
                            view.position = position;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        cx.on_release(|view, cx| {
            for image in view.frames.drain(..) {
                cx.drop_image(image, None);
            }
        })
        .detach();
        Self {
            player,
            config: ProgressConfig {
                duration: 0,
                track: None,
                theme: Theme::Forest,
                pet: ProgressPet::None,
                color: PetColor::Original,
                inactive: false,
            },
            frames: vec![],
            bounds: None,
            preview: None,
            position: 0,
            phase: 0,
            _timer: timer,
        }
    }
    pub fn configure(
        &mut self,
        config: ProgressConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if config == self.config {
            return;
        }
        if (config.pet, config.color) != (self.config.pet, self.config.color) {
            for image in self.frames.drain(..) {
                cx.drop_image(image, Some(window));
            }
            self.frames = pet_frames(config.pet, config.color);
        }
        if config.track != self.config.track {
            self.preview = None;
        }
        self.config = config;
        self.position = self.player.progress().2;
        cx.notify();
    }
    pub(super) fn is_dragging(&self) -> bool {
        self.preview.is_some()
    }
    pub(super) fn drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.config.duration == 0 {
            return;
        }
        if let Some(bounds) = self.bounds {
            if bounds.size.width <= px(0.) {
                return;
            }
            let value = ((position.x - bounds.left()) / bounds.size.width).clamp(0., 1.);
            self.preview = Some((value as f64 * self.config.duration as f64) as u64);
            cx.notify();
        }
    }
    pub(super) fn finish(&mut self, cx: &mut Context<Self>) {
        if let Some(target) = self.preview.take() {
            if self.player.progress().0 == self.config.track {
                self.player.seek(target);
                self.position = target;
            }
            cx.notify();
        }
    }
}

impl Render for PlaybackProgress {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.config.theme.palette();
        let position = self
            .preview
            .unwrap_or_else(|| self.player.progress().2)
            .min(self.config.duration);
        let value = if self.config.duration > 0 {
            position as f32 / self.config.duration as f32
        } else {
            0.
        };
        let weak = cx.weak_entity();
        let pet = !self.frames.is_empty();
        let mut bar = div()
            .relative()
            .h(px(22.))
            .flex_1()
            .min_w_0()
            .cursor_pointer()
            .flex()
            .items_center()
            .child(
                div()
                    .h(px(4.))
                    .w_full()
                    .rounded_full()
                    .bg(rgb(colors.border))
                    .child(
                        div()
                            .h_full()
                            .w(relative(value))
                            .rounded_full()
                            .bg(rgb(colors.accent)),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .top(px(7.))
                    .left(relative(value))
                    .ml(px(-4.))
                    .size_2()
                    .rounded_full()
                    .bg(rgb(colors.text)),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = weak.update(cx, |view, _| view.bounds = Some(bounds));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            );
        if pet {
            let index = self.phase as usize % self.frames.len();
            let mut animal = div()
                .absolute()
                .right_0()
                .bottom(px(12.))
                .w(px(64.))
                .h(px(48.))
                .child(img(self.frames[index].clone()).w(px(64.)).h(px(48.)));
            let listening = self.config.pet.listening(self.config.color);
            if self.config.pet.sleeping(self.config.color) || listening {
                for i in 0..3 {
                    let t = ((self.phase as f32 * 0.08 / 2.8) + i as f32 / 3.).fract();
                    let alpha = (std::f32::consts::PI * t).sin() * 0.8;
                    animal = animal.child(
                        div()
                            .absolute()
                            .right(px(5. - t * 6.))
                            .bottom(px(20. + t * 22.))
                            .text_size(px(8. + t * 3.))
                            .text_color(rgb(if listening {
                                colors.accent
                            } else {
                                colors.text
                            }))
                            .opacity(alpha)
                            .child(if listening {
                                if i % 2 == 0 { "♪" } else { "♫" }
                            } else {
                                "z"
                            }),
                    );
                }
            }
            if self.config.pet.smoking(self.config.color) {
                animal = animal.child(ember(self.phase as f32 * 0.08));
                for i in 0..3 {
                    let t = ((self.phase as f32 * 0.08 / 3.2) + i as f32 / 3.).fract();
                    animal = animal.child(
                        div()
                            .absolute()
                            .right(px(11. - (t * std::f32::consts::PI).sin() * 5.))
                            .bottom(px(21. + t * 24.))
                            .w(px(3. + t * 4.))
                            .h(px(5. + t * 7.))
                            .rounded_full()
                            .border_l_1()
                            .border_color(rgb(colors.muted))
                            .opacity((std::f32::consts::PI * t).sin() * 0.4),
                    );
                }
            }
            bar = bar.child(animal);
        }
        div()
            .id("playback-progress")
            .w_full()
            .min_w_0()
            .relative()
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
                if view.preview.is_some() {
                    view.drag(event.position, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _, _, cx| view.finish(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _, _, cx| view.finish(cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .child(div().w_10().child(format_duration(Some(position as i64))))
                    .child(bar.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.drag(event.position, cx);
                        }),
                    ))
                    .child(
                        div()
                            .w_10()
                            .child(format_duration(Some(self.config.duration as i64))),
                    ),
            )
    }
}

// Exactly eight 64x48 BGRA frames (96 KiB), loaded once per selected variant. No GIF
// decoder or frame allocation runs during animation; uploaded frames are reused.
fn pet_frames(pet: ProgressPet, color: PetColor) -> Vec<Arc<RenderImage>> {
    let Some(bytes) = pet.variant_asset(color) else {
        return vec![];
    };
    let Ok(image) = image::load_from_memory(bytes) else {
        return vec![];
    };
    let mut atlas = image.to_rgba8();
    for p in atlas.pixels_mut() {
        let [r, g, b, a] = p.0;
        // Recolor warm fur/feathers; preserve dark outlines, pink details and the nest.
        let fur = r > 75
            && g > 40
            && r >= g
            && g > b.saturating_add(12)
            && (pet != ProgressPet::Bird || r as u16 > g as u16 + 35);
        if !matches!(
            pet,
            ProgressPet::Bird | ProgressPet::Frog | ProgressPet::Pig
        ) && matches!(color, PetColor::Gray | PetColor::White | PetColor::Black)
            && fur
            && a > 0
        {
            let l = (r as f32 * 0.3 + g as f32 * 0.59 + b as f32 * 0.11) / 255.;
            let v = match color {
                PetColor::Gray => 40. + l * 150.,
                PetColor::White => 125. + l * 130.,
                PetColor::Black => 12. + l * 65.,
                _ => l * 255.,
            } as u8;
            p.0 = [v, v, v, a];
        }
        p.0.swap(0, 2);
    }
    (0..8)
        .map(|i| {
            let tile = image::imageops::crop_imm(&atlas, i * 128, 0, 128, 96).to_image();
            let filter =
                if matches!(pet, ProgressPet::Frog | ProgressPet::Pig) || pet.listening(color) {
                    image::imageops::FilterType::Nearest
                } else {
                    image::imageops::FilterType::Triangle
                };
            let frame = image::imageops::resize(&tile, 64, 48, filter);
            Arc::new(RenderImage::new(vec![image::Frame::new(frame)]))
        })
        .collect()
}

impl MusicApp {
    pub(super) fn progress_view(&self) -> AnyView {
        self.progress.clone().into()
    }
    pub(super) fn configure_progress(&self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.player.progress();
        let duration = state
            .0
            .and_then(|id| self.track_index.get(&id))
            .and_then(|i| self.tracks.get(*i))
            .and_then(|t| t.duration_ms)
            .unwrap_or(0)
            .max(0) as u64;
        let color = match self.preferences.progress_pet {
            ProgressPet::Cat => self.preferences.cat_color,
            ProgressPet::Bird => self.preferences.bird_color,
            ProgressPet::DancingCats => self.preferences.dancing_cats_color,
            ProgressPet::Frog => self.preferences.frog_variant,
            ProgressPet::Pig => self.preferences.pig_variant,
            ProgressPet::None => PetColor::Original,
        };
        self.progress.update(cx, |view, cx| {
            view.configure(
                ProgressConfig {
                    duration,
                    track: state.0,
                    theme: self.preferences.theme,
                    pet: self.preferences.progress_pet,
                    color: self.preferences.progress_pet.normalize_color(color),
                    inactive: self.preferences.refresh_inactive,
                },
                window,
                cx,
            )
        });
    }
}

// Previews exist only in the open pet settings. Seven variants together use at
// most 672 KiB of decoded sprite pixels and share one 12.5 Hz animation timer.
pub(super) struct PetPicker {
    parent: WeakEntity<MusicApp>,
    pet: ProgressPet,
    variants: Vec<(PetColor, Vec<Arc<RenderImage>>)>,
    theme: Theme,
    selected: PetColor,
    inactive: bool,
    frame: usize,
    _timer: Task<()>,
}
impl PetPicker {
    pub fn new(
        parent: WeakEntity<MusicApp>,
        pet: ProgressPet,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let variants = pet
            .variants()
            .iter()
            .map(|color| (*color, pet_frames(pet, *color)))
            .collect();
        let timer = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                if this
                    .update_in(cx, |view, window, cx| {
                        if window.is_window_active() || view.inactive {
                            view.frame = (view.frame + 1) % 240;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        cx.on_release(|view, cx| {
            for (_, frames) in &mut view.variants {
                for image in frames.drain(..) {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();
        Self {
            parent,
            pet,
            variants,
            theme,
            selected: PetColor::Original,
            inactive: false,
            frame: 0,
            _timer: timer,
        }
    }
}
impl Render for PetPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.theme.palette();
        div()
            .flex()
            .flex_wrap()
            .gap_3()
            .children(
                self.variants
                    .iter()
                    .enumerate()
                    .map(|(i, (color, frames))| {
                        let color = *color;
                        let parent = self.parent.clone();
                        let pet = self.pet;
                        div()
                            .id(("pet-animation-variant", i as u64))
                            .relative()
                            .w(px(112.))
                            .h(px(88.))
                            .rounded_lg()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_1()
                            .cursor_pointer()
                            .border_2()
                            .border_color(rgb(if color == self.selected {
                                colors.accent
                            } else {
                                colors.border
                            }))
                            .bg(rgb(colors.bg))
                            .hover(move |s| s.border_color(rgb(colors.accent)))
                            .children(frames.get(self.frame % 8).map(|image| {
                                div()
                                    .relative()
                                    .w(px(64.))
                                    .h(px(48.))
                                    .child(img(image.clone()).w(px(64.)).h(px(48.)))
                                    .children(
                                        pet.smoking(color).then(|| ember(self.frame as f32 * 0.08)),
                                    )
                            }))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(colors.muted))
                                    .child(pet.variant_label(color)),
                            )
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.selected = color;
                                let _ = parent.update(cx, |app, cx| {
                                    match pet {
                                        ProgressPet::Cat => app.preferences.cat_color = color,
                                        ProgressPet::Bird => app.preferences.bird_color = color,
                                        ProgressPet::DancingCats => {
                                            app.preferences.dancing_cats_color = color
                                        }
                                        ProgressPet::Frog => app.preferences.frog_variant = color,
                                        ProgressPet::Pig => app.preferences.pig_variant = color,
                                        ProgressPet::None => {}
                                    }
                                    app.save_preferences();
                                    cx.notify();
                                });
                                cx.notify();
                            }))
                    }),
            )
    }
}
impl MusicApp {
    pub(super) fn configure_pet_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let show = self.settings_open
            && self.settings_category == settings::SettingsCategory::Customization
            && self.customization_category == settings::CustomizationCategory::Pets
            && !self.now_playing_open
            && self.preferences.progress_pet != ProgressPet::None;
        if !show {
            self.pet_picker = None;
            return;
        }
        let pet = self.preferences.progress_pet;
        if self
            .pet_picker
            .as_ref()
            .is_none_or(|picker| picker.read(cx).pet != pet)
        {
            let parent = cx.weak_entity();
            let theme = self.preferences.theme;
            self.pet_picker = Some(cx.new(|cx| PetPicker::new(parent, pet, theme, window, cx)));
        }
        let color = match pet {
            ProgressPet::Cat => self.preferences.cat_color,
            ProgressPet::Bird => self.preferences.bird_color,
            ProgressPet::DancingCats => self.preferences.dancing_cats_color,
            ProgressPet::Frog => self.preferences.frog_variant,
            ProgressPet::Pig => self.preferences.pig_variant,
            ProgressPet::None => PetColor::Original,
        };
        self.pet_picker.as_ref().unwrap().update(cx, |view, cx| {
            if view.theme != self.preferences.theme
                || view.selected != color
                || view.inactive != self.preferences.refresh_inactive
            {
                view.theme = self.preferences.theme;
                view.selected = color;
                view.inactive = self.preferences.refresh_inactive;
                cx.notify();
            }
        });
    }
}
