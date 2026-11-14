use super::*;

#[derive(Clone, Copy)]
pub(super) enum SliderKind {
    Volume,
    Seek,
    BackgroundBlur,
    HeaderHue,
    HeaderSaturation,
    HeaderLightness,
}
impl SliderKind {
    fn is_header(self) -> bool {
        matches!(
            self,
            Self::HeaderHue | Self::HeaderSaturation | Self::HeaderLightness
        )
    }

    fn index(self) -> usize {
        match self {
            Self::Volume => 0,
            Self::Seek => 1,
            Self::BackgroundBlur => 2,
            Self::HeaderHue => 3,
            Self::HeaderSaturation => 4,
            Self::HeaderLightness => 5,
        }
    }
}

impl MusicApp {
    pub(super) fn playlist_artwork(&self, playlist: &Playlist, size: f32) -> AnyElement {
        let colors = self.colors();
        let mut cover = div()
            .size(px(size))
            .flex_shrink_0()
            .rounded_lg()
            .overflow_hidden()
            .bg(rgb(playlist
                .header_color
                .or(playlist.cover_color)
                .unwrap_or(colors.hero)));
        cover = if let Some(path) = &playlist.cover_path {
            cover.child(
                img(self.data_directory.join(path))
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .with_fallback(move || {
                        div()
                            .size_full()
                            .bg(rgb(colors.hero))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("♫")
                            .into_any_element()
                    }),
            )
        } else {
            cover
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(size * 0.4))
                .text_color(rgb(colors.text))
                .child("♫")
        };
        cover.into_any_element()
    }

    fn collection_header(&self, dense: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let playlist = self
            .selected_playlist
            .and_then(|id| self.playlists.iter().find(|p| p.id == id));
        let color = playlist
            .and_then(|p| p.header_color.or(p.cover_color))
            .unwrap_or(colors.hero);
        let shaded = [16, 8, 0].into_iter().fold(0, |result, shift| {
            (result << 8) | (((color >> shift) & 255) * 4 / 5)
        });
        let brightness =
            ((color >> 16) & 255) * 212 + ((color >> 8) & 255) * 715 + (color & 255) * 73;
        let heading_color = if brightness > 155_000 {
            0x101412
        } else {
            0xf5f5f7
        };
        let cover_size = if dense {
            64.
        } else if playlist.is_some() {
            152.
        } else {
            88.
        };
        let count = self.visible_tracks.len();
        let duration: i64 = self
            .visible_tracks
            .iter()
            .filter_map(|t| t.duration_ms)
            .sum();
        let mut cover = div()
            .id("collection-cover")
            .flex_shrink_0()
            .rounded_lg()
            .overflow_hidden();
        cover = if let Some(playlist) = playlist {
            let id = playlist.id;
            cover
                .cursor_pointer()
                .child(self.playlist_artwork(playlist, cover_size))
                .on_click(cx.listener(move |app, _, window, cx| {
                    app.open_modal(Modal::PlaylistAppearance(id), window, cx)
                }))
        } else {
            cover.child(
                div()
                    .size(px(cover_size))
                    .bg(rgb(colors.accent))
                    .text_color(rgb(colors.bg))
                    .text_size(px(cover_size * 0.5))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("♫"),
            )
        };
        div()
            .flex_shrink_0()
            .rounded_2xl()
            .overflow_hidden()
            .bg(linear_gradient(
                135.,
                linear_color_stop(self.surface(color), 0.),
                linear_color_stop(self.surface(shaded), 1.),
            ))
            .border_1()
            .border_color(rgb(colors.border))
            .p(px(if dense { 12. } else { 24. }))
            .flex()
            .items_center()
            .gap(px(if dense { 14. } else { 24. }))
            .child(cover)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(if dense { 4. } else { 10. }))
                    .children((!dense).then(|| {
                        div()
                            .text_xs()
                            .text_color(rgba((heading_color << 8) | 0xbb))
                            .child(if playlist.is_some() {
                                tr("ВАШ ПЛЕЙЛИСТ")
                            } else {
                                tr("МУЗЫКАЛЬНАЯ БИБЛИОТЕКА")
                            })
                    }))
                    .child(
                        div()
                            .text_size(px(if dense {
                                24.
                            } else if playlist.is_some() {
                                40.
                            } else {
                                34.
                            }))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(heading_color))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(self.current_title()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgba((heading_color << 8) | 0xbb))
                            .child(localized_format!(
                                "Треков: {count}  ·  {} мин",
                                "Tracks: {count}  ·  {} min",
                                duration / 60_000
                            )),
                    )
                    .children((playlist.is_some() && !dense).then(|| {
                        div()
                            .text_xs()
                            .text_color(rgba((heading_color << 8) | 0x99))
                            .child(tr("Нажмите на обложку, чтобы изменить оформление"))
                    })),
            )
            .children(playlist.map(|playlist| {
                let id = playlist.id;
                let actions = playlist.clone();
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(
                        self.icon_button("playlist-appearance", "✎", false)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.open_modal(Modal::PlaylistAppearance(id), window, cx)
                            })),
                    )
                    .child(
                        self.icon_button("playlist-menu", "⋯", false)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.open_modal(Modal::PlaylistActions(actions.clone()), window, cx)
                            })),
                    )
            }))
            .into_any_element()
    }

    pub(super) fn download_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let label = self.busy.as_ref()?;
        if matches!(self.modal, Some(Modal::Import)) {
            return None;
        }
        let colors = self.colors();
        let progress = self.job_progress.as_ref();
        let known =
            progress.is_some_and(|p| p.total > 1 || p.current_title.is_none() && p.total > 0);
        let fraction = progress
            .filter(|p| p.total > 0)
            .map(|p| p.completed as f32 / p.total as f32)
            .unwrap_or(0.);
        let phase = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            % 2400;
        let bar = div()
            .relative()
            .h(px(3.))
            .w_full()
            .rounded_full()
            .overflow_hidden()
            .bg(rgb(colors.border))
            .child(
                div()
                    .absolute()
                    .h_full()
                    .rounded_full()
                    .bg(rgb(colors.accent))
                    .left(relative(if known {
                        0.
                    } else {
                        phase as f32 / 2400. * 0.75
                    }))
                    .w(relative(if known { fraction.clamp(0., 1.) } else { 0.25 })),
            );
        let mut card = div()
            .id("download-card")
            .flex_shrink_0()
            .p_3()
            .rounded_lg()
            .bg(rgb(colors.input))
            .border_1()
            .border_color(rgb(colors.border))
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().text_xs().text_color(rgb(colors.text)).child(
                        if self.download_cancel.is_some() {
                            tr("↓ Загрузка")
                        } else {
                            tr("Обработка")
                        },
                    ))
                    .child(
                        div().text_xs().text_color(rgb(colors.muted)).child(
                            progress
                                .filter(|p| p.total > 0)
                                .map(|p| format!("{}/{}", p.completed, p.total))
                                .unwrap_or("…".into()),
                        ),
                    ),
            )
            .child(
                div()
                    .my_2()
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .text_ellipsis()
                    .overflow_hidden()
                    .child(
                        progress
                            .and_then(|p| p.current_title.clone())
                            .unwrap_or(label.clone()),
                    ),
            )
            .child(bar)
            .on_click(cx.listener(|app, _, window, cx| app.open_modal(Modal::Import, window, cx)));
        if let Some(cancel) = &self.download_cancel {
            let token = cancel.clone();
            card = card.child(
                div()
                    .id("cancel-mini-download")
                    .mt_2()
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .child(if cancel.load(Ordering::Relaxed) {
                        tr("Останавливаем…")
                    } else {
                        tr("Остановить")
                    })
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.stop_propagation();
                        token.store(true, Ordering::Relaxed);
                        cx.notify();
                    })),
            );
        }
        Some(card.into_any_element())
    }

    pub(super) fn notifications(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let colors = self.colors();
        if self.message.is_none() && self.error.is_none() {
            return None;
        }
        let has_error = self.error.is_some();
        let mut heading = div()
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .text_color(rgb(if has_error { 0xf0ba82 } else { colors.accent }))
                    .child(if has_error { "!" } else { "✓" }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(
                        self.message
                            .clone()
                            .unwrap_or(tr("Не удалось выполнить действие").into()),
                    ),
            );
        if has_error {
            heading = heading.child(
                div()
                    .id("error-details")
                    .text_xs()
                    .cursor_pointer()
                    .text_color(rgb(colors.muted))
                    .hover(move |s| s.text_color(rgb(colors.text)))
                    .child(if self.error_details_open {
                        tr("Свернуть")
                    } else {
                        tr("Подробнее")
                    })
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.error_details_open = !app.error_details_open;
                        cx.notify();
                    })),
            );
        }
        heading = heading.child(
            self.icon_button("close-notification", "×", false)
                .on_click(cx.listener(|app, _, _, cx| app.close_notifications(cx))),
        );
        let mut banner = div()
            .rounded_lg()
            .bg(rgb(colors.button))
            .border_1()
            .border_color(rgb(colors.border))
            .px_3()
            .py_2()
            .child(heading);
        if self.error_details_open
            && let Some(error) = &self.error
        {
            banner = banner.child(
                div()
                    .id("notification-details")
                    .mt_3()
                    .max_h(px(128.))
                    .overflow_y_scroll()
                    .text_xs()
                    .text_color(rgb(0xf1b2a5))
                    .child(error.clone()),
            );
        }
        Some(banner.into_any_element())
    }

    fn track_row(
        &self,
        track: Track,
        index: usize,
        state: &crate::player::PlaybackState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = self.colors();
        let active = state.track_id == Some(track.id);
        let id = track.id;
        let clicked = track.clone();
        let menu = track.clone();
        let add = track.clone();
        div()
            .id(("track", id as u64))
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(if active {
                rgb(colors.border)
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .bg(if active {
                self.surface(colors.active)
            } else if self.active_background().is_some() {
                rgba((colors.bg << 8) | 0x35).into()
            } else {
                rgb(colors.bg).into()
            })
            .hover(move |s| s.border_color(rgb(colors.hover_border)))
            .child(
                div()
                    .w_7()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(rgb(if active { colors.accent } else { colors.muted }))
                    .child(if active {
                        if state.paused {
                            "▶".into()
                        } else {
                            "Ⅱ".into()
                        }
                    } else {
                        format!("{:02}", index + 1)
                    }),
            )
            .child(self.artwork(index, 40.))
            .child(self.track_caption(&track, active))
            .child(
                div()
                    .w_12()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(rgb(colors.muted))
                    .child(format_duration(track.duration_ms)),
            )
            .child(
                self.icon_button(("add-track", id as u64), "+", false)
                    .on_click(cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        app.open_modal(Modal::AddToPlaylist(add.clone()), window, cx);
                    })),
            )
            .child(
                self.icon_button(("track-menu", id as u64), "⋯", false)
                    .on_click(cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        app.open_modal(Modal::TrackActions(menu.clone()), window, cx);
                    })),
            )
            .on_click(cx.listener(move |app, _, _, cx| app.play(clicked.clone(), cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.open_modal(Modal::TrackActions(track.clone()), window, cx);
                }),
            )
            .into_any_element()
    }

    fn queue_panel(&self, overlay: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let state = self.player.state();
        let current = self
            .tracks
            .iter()
            .find(|track| Some(track.id) == state.track_id);
        let mut panel = div()
            .id("queue-panel")
            .w(px(288.))
            .h_full()
            .flex_shrink_0()
            .bg(self.surface(colors.panel))
            .border_l_1()
            .border_color(rgb(colors.border))
            .p_5()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr("Очередь")),
                    )
                    .child(
                        self.icon_button("hide-queue", "×", false)
                            .on_click(cx.listener(|app, _, _, cx| app.toggle_queue(cx))),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .child(tr("СЕЙЧАС ИГРАЕТ")),
            );
        if let Some(track) = current {
            panel = panel.child(
                div()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(colors.active))
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.artwork(0, 42.))
                    .child(self.track_caption(track, true)),
            );
        } else {
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(rgb(colors.muted))
                    .child(tr("Выберите трек или нажмите «Слушать»")),
            );
        }
        panel =
            panel
                .child(div().mt_2().text_xs().text_color(rgb(colors.muted)).child(
                    localized_format!("ДАЛЕЕ · {}", "UP NEXT · {}", state.queue.upcoming().len()),
                ))
                .child(
                    div()
                        .text_sm()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(self.playback_context.clone()),
                );
        if state.queue.upcoming().is_empty() {
            panel = panel.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap_3()
                    .child(
                        div()
                            .text_color(rgb(colors.muted))
                            .text_sm()
                            .child(tr("В очереди пока пусто")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors.muted))
                            .child(tr("Выберите подборку или добавьте трек через меню ⋯")),
                    ),
            );
        } else {
            let queue = state.queue.clone();
            panel = panel.child(
                uniform_list(
                    "queue-scroll",
                    queue.upcoming().len(),
                    cx.processor(move |app, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| {
                                let id = queue.upcoming()[index];
                                let mut row = div()
                                    .id(("queued", id as u64))
                                    .w_full()
                                    .h(px(64.))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .p_2()
                                    .rounded_lg()
                                    .border_1()
                                    .border_color(rgb(colors.panel))
                                    .cursor_pointer()
                                    .hover(move |s| s.border_color(rgb(colors.hover_border)))
                                    .child(
                                        div()
                                            .w_5()
                                            .text_xs()
                                            .text_color(rgb(colors.muted))
                                            .child((index + 1).to_string()),
                                    );
                                if let Some(track) = app
                                    .track_index
                                    .get(&id)
                                    .and_then(|index| app.tracks.get(*index))
                                {
                                    row = row.child(app.track_caption(track, false));
                                }
                                row.child(
                                    app.icon_button(("unqueue", id as u64), "×", false)
                                        .on_click(cx.listener(move |app, _, _, cx| {
                                            cx.stop_propagation();
                                            app.player.remove(id);
                                        })),
                                )
                                .on_click(cx.listener(move |app, _, _, _| app.player.select(id)))
                            })
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0(),
            );
        }
        panel = panel.child(
            div()
                .border_t_1()
                .border_color(rgb(colors.border))
                .pt_3()
                .text_xs()
                .text_color(rgb(colors.muted))
                .child(if self.preferences.shuffle {
                    tr("⤨  Перемешивание включено")
                } else {
                    tr("В порядке выбранной подборки")
                }),
        );
        if overlay {
            panel = panel
                .absolute()
                .right_0()
                .top_0()
                .bottom_0()
                .shadow_lg()
                .occlude();
        }
        panel.into_any_element()
    }

    pub(super) fn slider(
        &self,
        id: &'static str,
        kind: SliderKind,
        value: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = self.colors();
        let weak = cx.weak_entity();
        let header = kind.is_header();
        let color = self.header_color_state(self.background_scope());
        let stops: Vec<Hsla> = match kind {
            SliderKind::HeaderHue => (0..=6)
                .map(|step| hsla(step as f32 / 6., 1., 0.5, 1.))
                .collect(),
            SliderKind::HeaderSaturation => vec![
                hsla(color.h, 0., color.l, 1.),
                hsla(color.h, 1., color.l, 1.),
            ],
            SliderKind::HeaderLightness => vec![
                hsla(0., 0., 0., 1.),
                hsla(color.h, color.s, 0.5, 1.),
                hsla(0., 0., 1., 1.),
            ],
            _ => Vec::new(),
        };
        div()
            .id(id)
            .relative()
            .h(px(20.))
            .w_full()
            .cursor_pointer()
            .flex()
            .items_center()
            .child(
                div()
                    .h(px(if header { 12. } else { 4. }))
                    .w_full()
                    .rounded_full()
                    .bg(rgb(colors.border))
                    .flex()
                    .overflow_hidden()
                    .children(stops.windows(2).map(|pair| {
                        div().h_full().flex_1().bg(linear_gradient(
                            90.,
                            linear_color_stop(pair[0], 0.),
                            linear_color_stop(pair[1], 1.),
                        ))
                    }))
                    .children((!header).then(|| {
                        div()
                            .h_full()
                            .w(relative(value.clamp(0., 1.)))
                            .rounded_full()
                            .bg(rgb(colors.accent))
                    })),
            )
            .child(
                div()
                    .absolute()
                    .top(px(6.))
                    .left(relative(value.clamp(0., 1.)))
                    .ml(px(-4.))
                    .size_2()
                    .rounded_full()
                    .bg(rgb(colors.text)),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = weak
                            .update(cx, |app, _| app.slider_bounds[kind.index()] = Some(bounds));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if kind.is_header() && app.busy.is_some() {
                        return;
                    }
                    app.dragging_slider = Some(kind);
                    app.change_slider(kind, event.position, cx);
                }),
            )
            .on_scroll_wheel(cx.listener(move |app, event: &ScrollWheelEvent, _, cx| {
                if matches!(kind, SliderKind::Volume) {
                    let delta = event.delta.pixel_delta(px(20.)).y / px(20.);
                    let step = if event.modifiers.shift { 0.001 } else { 0.01 };
                    if delta != 0. {
                        app.set_volume(app.preferences.volume + delta.signum() * step, cx);
                        app.save_preferences();
                    }
                    cx.stop_propagation();
                }
            }))
            .into_any_element()
    }

    pub(super) fn change_slider(
        &mut self,
        kind: SliderKind,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.slider_bounds[kind.index()] else {
            return;
        };
        if bounds.size.width <= px(0.) {
            return;
        }
        let value = ((position.x - bounds.left()) / bounds.size.width).clamp(0., 1.);
        match kind {
            SliderKind::HeaderHue | SliderKind::HeaderSaturation | SliderKind::HeaderLightness => {
                let id = self.background_scope();
                let mut color = self.header_color_state(id);
                match kind {
                    SliderKind::HeaderHue => color.h = value,
                    SliderKind::HeaderSaturation => color.s = value,
                    _ => color.l = value,
                }
                self.header_color_draft = Some((id, color));
                self.input.update(cx, |input, cx| {
                    input.set_value(&format!("#{:06X}", Self::header_color_value(color)));
                    cx.notify();
                });
                cx.notify();
            }
            SliderKind::Volume => self.set_volume(value, cx),
            SliderKind::BackgroundBlur => {
                self.blur_preview = Some((value * 40.).round() as u8);
                cx.notify();
            }
            SliderKind::Seek => {
                let current = self.player.progress().0;
                if let Some(duration) = self
                    .tracks
                    .iter()
                    .find(|track| Some(track.id) == current)
                    .and_then(|track| track.duration_ms)
                {
                    self.seek_preview = Some((duration.max(0) as f64 * value as f64) as u64);
                    cx.notify();
                }
            }
        }
    }

    fn finish_slider(&mut self, cx: &mut Context<Self>) {
        match self.dragging_slider.take() {
            Some(SliderKind::BackgroundBlur) => {
                if let Some(blur) = self.blur_preview.take() {
                    self.apply_background_blur(self.background_scope(), blur, cx);
                }
            }
            Some(
                SliderKind::HeaderHue | SliderKind::HeaderSaturation | SliderKind::HeaderLightness,
            ) => {
                if let Some((id, color)) = self.header_color_draft {
                    self.set_playlist_color(id, Some(Self::header_color_value(color)), cx);
                }
            }
            Some(SliderKind::Volume) => self.save_preferences(),
            Some(SliderKind::Seek) => {
                if let Some(position) = self.seek_preview.take() {
                    self.player.seek(position);
                }
            }
            None => {}
        }
        cx.notify();
    }

    fn footer(&self, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let state = self.player.state();
        let current = self
            .tracks
            .iter()
            .find(|track| Some(track.id) == state.track_id);
        let position = self.seek_preview.unwrap_or(state.position_ms);
        let duration = current
            .and_then(|track| track.duration_ms)
            .unwrap_or(0)
            .max(0) as u64;
        let progress = if duration > 0 {
            position as f32 / duration as f32
        } else {
            0.
        };
        let mut caption = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap_3()
            .child(self.artwork(0, if compact { 36. } else { 48. }));
        caption = if let Some(track) = current {
            caption.child(self.track_caption(track, false))
        } else {
            caption.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().child(tr("Музыка всегда рядом")))
                    .child(
                        div()
                            .text_xs()
                            .mt_1()
                            .text_color(rgb(colors.muted))
                            .child(tr("Выберите трек в библиотеке")),
                    ),
            )
        };
        let transport = div()
            .w_full()
            .max_w(px(560.))
            .min_w_0()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(
                        self.icon_button("shuffle-player", "⤨", self.preferences.shuffle)
                            .on_click(cx.listener(|app, _, _, cx| app.toggle_shuffle(cx))),
                    )
                    .child(
                        self.icon_button("previous", "‹‹", false)
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.player.previous();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("toggle-play")
                            .size_10()
                            .rounded_full()
                            .bg(rgb(colors.accent))
                            .text_color(rgb(colors.bg))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_lg()
                            .child(if current.is_some() && !state.paused {
                                "Ⅱ"
                            } else {
                                "▶"
                            })
                            .on_click(cx.listener(|app, _, _, cx| app.toggle_playback(cx))),
                    )
                    .child(self.icon_button("next", "››", false).on_click(cx.listener(
                        |app, _, _, cx| {
                            app.player.next();
                            cx.notify();
                        },
                    )))
                    .child(
                        self.icon_button("stop-player", "■", false)
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.player.stop();
                                cx.notify();
                            })),
                    )
                    .child(
                        self.icon_button(
                            "repeat-player",
                            match self.preferences.repeat {
                                RepeatMode::Off => tr("↻ Выкл"),
                                RepeatMode::All => tr("↻ Всё"),
                                RepeatMode::One => tr("↻ Трек"),
                            },
                            self.preferences.repeat != RepeatMode::Off,
                        )
                        .w(px(76.))
                        .text_xs()
                        .on_click(cx.listener(|app, _, _, cx| app.cycle_repeat(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .child(div().w_10().child(format_duration(Some(position as i64))))
                    .child(div().flex_1().child(self.slider(
                        "seek-slider",
                        SliderKind::Seek,
                        progress,
                        cx,
                    )))
                    .child(
                        div()
                            .w_10()
                            .child(format_duration(current.and_then(|track| track.duration_ms))),
                    ),
            );
        let volume_tools = div()
            .w(px(if compact { 264. } else { 280. }))
            .max_w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id("toggle-queue")
                    .px_2()
                    .py_2()
                    .rounded_lg()
                    .text_xs()
                    .cursor_pointer()
                    .text_color(rgb(if self.preferences.queue_visible {
                        colors.accent
                    } else {
                        colors.muted
                    }))
                    .hover(move |s| s.bg(rgb(colors.button_hover)))
                    .child(tr("≡ Очередь"))
                    .on_click(cx.listener(|app, _, _, cx| app.toggle_queue(cx))),
            )
            .child(
                self.icon_button(
                    "mute",
                    if self.preferences.volume == 0. {
                        "♩"
                    } else {
                        "♪"
                    },
                    self.preferences.volume == 0.,
                )
                .on_click(cx.listener(|app, _, _, cx| {
                    let volume = if app.preferences.volume == 0. {
                        app.last_volume
                    } else {
                        0.
                    };
                    app.set_volume(volume, cx);
                    app.save_preferences();
                })),
            )
            .child(div().flex_1().min_w_0().child(self.slider(
                "volume-slider",
                SliderKind::Volume,
                self.preferences.volume,
                cx,
            )))
            .child(div().w_12().text_xs().text_color(rgb(colors.muted)).child(
                if self.preferences.volume > 0. && self.preferences.volume < 0.1 {
                    format!("{:.1}%", self.preferences.volume * 100.)
                } else {
                    format!("{:.0}%", self.preferences.volume * 100.)
                },
            ));
        let footer = div()
            .flex_shrink_0()
            .bg(self.surface(colors.panel))
            .border_t_1()
            .border_color(rgb(colors.border))
            .px(px(if compact { 16. } else { 24. }));
        if compact {
            footer
                .h(px(128.))
                .py_2()
                .flex()
                .flex_col()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(caption)
                        .child(volume_tools),
                )
                .child(transport)
                .into_any_element()
        } else {
            // Equal side columns keep play/pause exactly on the window centre,
            // independent of the current title or volume controls' width.
            footer
                .h(px(104.))
                .flex()
                .items_center()
                .gap_5()
                .child(caption)
                .child(
                    div()
                        .w(relative(0.44))
                        .max_w(px(560.))
                        .flex_shrink_0()
                        .child(transport),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .justify_end()
                        .child(volume_tools),
                )
                .into_any_element()
        }
    }
}

impl MusicApp {
    // The header is the first item of the virtual list, so it scrolls away with the tracks.
    fn collection_intro(
        &self,
        viewport: Size<Pixels>,
        empty: bool,
        state: &crate::player::PlaybackState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = self.colors();
        let compact = viewport.width < px(1100.);
        let dense = compact || viewport.height < px(700.);
        let short = viewport.height < px(620.);
        let active_context =
            state.track_id.is_some() && self.playback_playlist == self.selected_playlist;
        let controls = div()
            .flex_shrink_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child(
                self.button(
                    "play-collection",
                    if active_context && !state.paused {
                        tr("Ⅱ  Пауза")
                    } else {
                        tr("▶  Слушать")
                    },
                    true,
                )
                .on_click(cx.listener(|app, _, _, cx| app.play_collection(cx))),
            )
            .child(
                self.button(
                    "shuffle-collection",
                    tr("⤨  Перемешать"),
                    self.preferences.shuffle,
                )
                .on_click(cx.listener(|app, _, _, cx| app.shuffle_collection(cx))),
            );
        let mut content = div()
            .id("collection-content")
            .w_full()
            .max_w(px(1120.))
            .min_w_0()
            .flex_shrink_0()
            .pt(px(if dense { 8. } else { 28. }))
            .pb(px(if dense { 8. } else { 16. }))
            .flex()
            .flex_col()
            .gap(px(if dense { 8. } else { 16. }))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .rounded_lg()
                            .overflow_hidden()
                            .child(self.search.clone()),
                    )
                    .child(
                        self.button(
                            "import",
                            if compact {
                                tr("+  Музыка")
                            } else {
                                tr("+  Добавить музыку")
                            },
                            true,
                        )
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.open_modal(Modal::Import, window, cx)
                        })),
                    ),
            )
            .child(self.collection_header(dense, cx))
            .child(controls);
        content = content.children(self.notifications(cx));
        content = content.child(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_between()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(tr("Треки")))
                .child(div().text_xs().text_color(rgb(colors.muted)).child(
                    if self.selected_playlist.is_some() {
                        tr("В порядке плейлиста")
                    } else {
                        tr("Недавно добавленные")
                    },
                )),
        );
        if empty {
            let searching = !self.search.read(cx).value().is_empty();
            content = content.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_4()
                    .child(
                        div()
                            .text_size(px(44.))
                            .text_color(rgb(colors.muted))
                            .child("♫"),
                    )
                    .child(div().text_lg().child(if searching {
                        tr("Ничего не найдено")
                    } else {
                        tr("Здесь будет ваша музыка")
                    }))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(colors.muted))
                            .child(if searching {
                                tr("Попробуйте другое название или исполнителя")
                            } else {
                                tr("Добавьте музыку или выберите треки из библиотеки")
                            }),
                    ),
            );
        } else {
            content = content.children((!short).then(|| {
                div()
                    .flex_shrink_0()
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_3()
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .child(div().w_7().child("#"))
                    .child(div().flex_1().child(tr("НАЗВАНИЕ / ИСПОЛНИТЕЛЬ")))
                    .child(div().w_12().child(tr("ВРЕМЯ")))
                    .child(div().w(px(76.)))
            }));
        }
        content.into_any_element()
    }
}

impl Render for MusicApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors();
        let viewport = window.viewport_size();
        let compact = viewport.width < px(1100.);
        let dense = compact || viewport.height < px(700.);
        let queue_overlay = viewport.width < px(1280.);
        let state = self.player.state();
        let query = self.search.read(cx).value();
        if self.collection_dirty
            || self.collection_query != query
            || self.scroll_context != self.selected_playlist
        {
            let tracks = self.filtered_tracks(cx);
            let ids: Vec<_> = tracks.iter().map(|track| track.id).collect();
            if self.scroll_context != self.selected_playlist
                || self.collection_query != query
                || self.collection_items != ids
            {
                self.collection_list.reset(tracks.len() + 1);
            }
            self.scroll_context = self.selected_playlist;
            self.collection_query = query;
            self.collection_items = ids;
            self.collection_tracks = Rc::new(tracks);
            self.collection_dirty = false;
        }
        let tracks = self.collection_tracks.clone();
        let empty = tracks.is_empty();
        let content = list(
            self.collection_list.clone(),
            cx.processor(move |app, index: usize, _, cx| {
                if index == 0 {
                    app.collection_intro(viewport, empty, &state, cx)
                } else {
                    app.track_row(tracks[index - 1].clone(), index - 1, &state, cx)
                }
            }),
        )
        .size_full()
        .max_w(px(1120.))
        .min_h_0();
        let mut body = div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .child(self.sidebar(compact || viewport.height < px(700.), cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .justify_center()
                    .px(px(if dense { 16. } else { 32. }))
                    .child(if self.settings_open {
                        self.settings_view(cx)
                    } else if self.home_open {
                        self.home_view(viewport, cx)
                    } else {
                        content.into_any_element()
                    })
                    .children(
                        (!self.home_open && !self.settings_open && {
                            let top = self.collection_list.logical_scroll_top();
                            top.item_ix > 0 || top.offset_in_item > px(180.)
                        })
                        .then(|| {
                            div()
                                .id("back-to-top")
                                .absolute()
                                .right_5()
                                .bottom_5()
                                .size_11()
                                .rounded_full()
                                .occlude()
                                .bg(rgb(colors.accent))
                                .text_color(rgb(colors.bg))
                                .shadow_lg()
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_xl()
                                .child("↑")
                                .on_click(cx.listener(|app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.collection_list.scroll_to(ListOffset::default());
                                    cx.notify();
                                }))
                        }),
                    ),
            );
        if self.preferences.queue_visible {
            if queue_overlay {
                body = body.child(
                    div()
                        .id("queue-backdrop")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .bg(rgba(0x05090688))
                        .on_click(cx.listener(|app, _, _, cx| app.toggle_queue(cx))),
                );
            }
            body = body.child(self.queue_panel(queue_overlay, cx));
        }
        div()
            .relative()
            .size_full()
            .bg(rgb(colors.bg))
            .text_color(rgb(colors.text))
            .font_family("Segoe UI")
            .text_size(px(14.))
            .track_focus(&self.focus)
            .key_context("MusicApp")
            .on_action(cx.listener(Self::dismiss))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(|app, _: &TogglePlayback, _, cx| app.toggle_playback(cx)))
            .on_action(cx.listener(|app, _: &NextTrack, _, cx| {
                app.player.next();
                cx.notify();
            }))
            .on_action(cx.listener(|app, _: &PreviousTrack, _, cx| {
                app.player.previous();
                cx.notify();
            }))
            .on_action(cx.listener(|app, _: &ToggleQueue, _, cx| app.toggle_queue(cx)))
            .on_action(cx.listener(|app, _: &VolumeUp, _, cx| {
                app.set_volume(app.preferences.volume + 0.01, cx);
                app.save_preferences();
            }))
            .on_action(cx.listener(|app, _: &VolumeDown, _, cx| {
                app.set_volume(app.preferences.volume - 0.01, cx);
                app.save_preferences();
            }))
            .on_mouse_move(cx.listener(|app, event: &MouseMoveEvent, _, cx| {
                if let Some(kind) = app.dragging_slider {
                    app.change_slider(kind, event.position, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _, _, cx| app.finish_slider(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|app, _, _, cx| app.finish_slider(cx)),
            )
            .flex()
            .flex_col()
            .children(self.active_background().map(|background| {
                div()
                    .absolute()
                    .inset_0()
                    .overflow_hidden()
                    .child(
                        img(self.data_directory.join(&background.display_path))
                            .size_full()
                            .object_fit(ObjectFit::Cover),
                    )
                    .child(div().absolute().inset_0().bg(rgba((colors.bg << 8) | 0x94)))
            }))
            .child(body)
            .child(self.footer(compact, cx))
            .children(self.modal_view(viewport, cx))
    }
}

impl MusicApp {
    pub(super) fn icon_button(
        &self,
        id: impl Into<ElementId>,
        label: &str,
        active: bool,
    ) -> Stateful<Div> {
        let colors = self.colors();
        div()
            .id(id)
            .size_8()
            .flex_shrink_0()
            .rounded_lg()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_size(px(18.))
            .text_color(rgb(if active { colors.accent } else { colors.muted }))
            .bg(rgb(if active { colors.active } else { colors.panel }))
            .hover(move |s| s.bg(rgb(colors.button_hover)).text_color(rgb(colors.text)))
            .child(label.to_owned())
    }

    fn artwork(&self, index: usize, size: f32) -> Div {
        let colors = self.colors();
        div()
            .size(px(size))
            .flex_shrink_0()
            .rounded_md()
            .bg(rgb(match self.preferences.theme {
                Theme::Forest => [0x344735, 0x39354a, 0x49402d, 0x29454a],
                Theme::Onyx => [0x292b34, 0x302d39, 0x343139, 0x283137],
                Theme::PickMe => [0x56314c, 0x4b344d, 0x63394e, 0x4b3040],
                Theme::Cosmic => [0x302b59, 0x253957, 0x40325c, 0x29374e],
            }[index % 4]))
            .flex()
            .items_center()
            .justify_center()
            .text_lg()
            .text_color(rgb(colors.text))
            .child("♫")
    }

    fn track_caption(&self, track: &Track, _active: bool) -> Div {
        let colors = self.colors();
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(rgb(colors.text))
                    .child(track.title.clone()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(if track.artist.is_empty() {
                        tr("Неизвестный исполнитель").into()
                    } else {
                        track.artist.clone()
                    }),
            )
    }
}
