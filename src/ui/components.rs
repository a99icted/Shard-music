use super::*;

#[derive(Clone, Copy)]
pub(super) enum SliderKind {
    Volume,
    BackgroundBlur,
    HeaderHue,
    HeaderSaturation,
    HeaderLightness,
    CursorPetSpeed,
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
            Self::BackgroundBlur => 1,
            Self::HeaderHue => 2,
            Self::HeaderSaturation => 3,
            Self::HeaderLightness => 4,
            Self::CursorPetSpeed => 5,
        }
    }
}

impl MusicApp {
    /// Paint-only feedback keeps button geometry and event identity stable.
    /// Animation frames stop as soon as the short hover/press transition finishes.
    pub(super) fn button_feedback(&self, radius: f32) -> AnyElement {
        #[derive(Default)]
        struct Feedback {
            hover: f32,
            last: Option<Instant>,
            press: Option<Instant>,
        }
        div()
            .absolute()
            .inset_0()
            .child(
                canvas(
                    |bounds, window, cx| {
                        let state = window.use_keyed_state("button-feedback-state", cx, |_, _| {
                            Feedback::default()
                        });
                        let hover =
                            window.is_window_active() && bounds.contains(&window.mouse_position());
                        let alpha = state.update(cx, |s, _| {
                            let now = Instant::now();
                            let dt = s
                                .last
                                .map_or(1., |last| now.duration_since(last).as_secs_f32());
                            s.last = Some(now);
                            let target = if hover { 1. } else { 0. };
                            let step = (dt / 0.14).min(1.);
                            s.hover += (target - s.hover).clamp(-step, step);
                            let pulse = s.press.map_or(0., |at| {
                                (1. - now.duration_since(at).as_secs_f32() / 0.2).max(0.)
                            });
                            if (target - s.hover).abs() > 0.001 || pulse > 0. {
                                window.request_animation_frame();
                            }
                            0.065 * s.hover + 0.1 * pulse
                        });
                        (state, alpha)
                    },
                    move |bounds, (state, alpha), window, _cx| {
                        if alpha > 0.001 {
                            let mut quad = fill(bounds, rgba(0xffffff00 | (alpha * 255.) as u32));
                            quad.corner_radii = px(radius).into();
                            window.paint_quad(quad);
                        }
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble
                                && event.button == MouseButton::Left
                                && bounds.contains(&event.position)
                            {
                                state.update(cx, |s, _| s.press = Some(Instant::now()));
                                window.refresh();
                            }
                        });
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }

    pub(super) fn brand_mark(&self, size: f32) -> AnyElement {
        img(self.preferences.theme.logo_path())
            .image_cache(&self.images)
            .size(px(size))
            .flex_shrink_0()
            .into_any_element()
    }

    pub(super) fn library_artwork(&self, size: f32) -> AnyElement {
        let colors = self.colors();
        div()
            .size(px(size))
            .flex_shrink_0()
            .rounded_2xl()
            .overflow_hidden()
            .bg(linear_gradient(
                135.,
                linear_color_stop(rgb(colors.accent), 0.),
                linear_color_stop(rgb(colors.hero), 1.),
            ))
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path("icons/library-cover.svg")
                    .size(px(size * 0.58))
                    .text_color(rgb(colors.text)),
            )
            .into_any_element()
    }

    pub(super) fn playback_mode_control(
        &self,
        id: &'static str,
        size: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = self.colors();
        let mode = self.playback_mode();
        div()
            .id(id)
            .relative()
            .size(px(size))
            .flex_shrink_0()
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(rgb(if mode == shard::queue::PlaybackMode::Ordered {
                colors.muted
            } else {
                colors.accent
            }))
            .hover(move |s| s.bg(rgb(colors.button_hover)))
            .active(|s| s.opacity(0.65))
            .tooltip(move |_, cx| {
                cx.new(|_| super::sidebar::SidebarTooltip(mode.label().into()))
                    .into()
            })
            .child(
                svg()
                    .path(mode.icon())
                    .size(px(size * 0.55))
                    .with_animation(
                        ("mode-change", mode as u64),
                        Animation::new(Duration::from_millis(180)).with_easing(ease_out_quint()),
                        |icon, delta| {
                            icon.with_transformation(
                                Transformation::rotate(radians(-0.35 * (1. - delta)))
                                    .with_scaling(gpui::size(0.8 + 0.2 * delta, 0.8 + 0.2 * delta)),
                            )
                        },
                    ),
            )
            .on_click(cx.listener(|app, _, _, cx| app.cycle_playback_mode(cx)))
            .child(self.button_feedback(size / 2.))
            .into_any_element()
    }

    pub(super) fn playlist_artwork(&self, playlist: &Playlist, size: f32) -> AnyElement {
        let colors = self.colors();
        if playlist.id == shard::favorites::FAVORITES_ID {
            return div()
                .size(px(size))
                .flex_shrink_0()
                .rounded_2xl()
                .overflow_hidden()
                .bg(linear_gradient(
                    135.,
                    linear_color_stop(rgb(colors.accent), 0.),
                    linear_color_stop(rgb(colors.hero), 1.),
                ))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path("icons/favorites-cover.svg")
                        .size(px(size * 0.78))
                        .text_color(rgb(colors.text)),
                )
                .into_any_element();
        }
        let mut cover = div()
            .size(px(size))
            .flex_shrink_0()
            .rounded_lg()
            .overflow_hidden()
            .bg(rgb(playlist
                .header_color
                .or(playlist.cover_color)
                .unwrap_or(colors.hero)));
        cover = if let Some(path) = &playlist.cover_path
            && (size > 96. || self.preview_paths.contains(&self.data_directory.join(path)))
        {
            cover.child(
                img(if size <= 96. {
                    shard::artwork::thumbnail_path(&self.data_directory.join(path))
                } else {
                    self.data_directory.join(path)
                })
                .image_cache(&self.images)
                .size_full()
                .object_fit(ObjectFit::Cover)
                .with_fallback(move || {
                    div()
                        .size_full()
                        .bg(rgb(colors.hero))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            svg()
                                .path("icons/playlist.svg")
                                .size(px(size * 0.52))
                                .text_color(rgb(colors.text)),
                        )
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
                .child(svg().path("icons/playlist.svg").size(px(size * 0.52)))
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
        let regular_playlist = playlist.is_some_and(|p| p.id != shard::favorites::FAVORITES_ID);
        let cover_size = if dense {
            64.
        } else if regular_playlist {
            152.
        } else {
            88.
        };
        let count = self.visible_tracks.len();
        let duration = self.visible_duration_ms;
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
                    if id != shard::favorites::FAVORITES_ID {
                        app.open_modal(Modal::PlaylistAppearance(id), window, cx);
                    }
                }))
        } else {
            cover.child(self.library_artwork(cover_size))
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
                            } else if regular_playlist {
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
                    .children(
                        (playlist.is_some_and(|p| p.id != shard::favorites::FAVORITES_ID)
                            && !dense)
                            .then(|| {
                                div()
                                    .text_xs()
                                    .text_color(rgba((heading_color << 8) | 0x99))
                                    .child(tr("Нажмите на обложку, чтобы изменить оформление"))
                            }),
                    ),
            )
            .children(playlist.map(|playlist| {
                let id = playlist.id;
                let actions = playlist.clone();
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .children((id != shard::favorites::FAVORITES_ID).then(|| {
                        self.icon_button("playlist-appearance", "✎", false)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.open_modal(Modal::PlaylistAppearance(id), window, cx)
                            }))
                    }))
                    .child(
                        self.icon_button("playlist-menu", "⋯", false)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.open_modal(Modal::PlaylistActions(actions.clone()), window, cx)
                            })),
                    )
            }))
            .children(
                (playlist.is_none() && self.sync_service.is_some()).then(|| {
                    self.button(
                        "sync-library-header",
                        tr("Синхронизировать все треки"),
                        false,
                    )
                    .on_click(cx.listener(|app, _, window, cx| {
                        app.request_sync(shard::sync::SyncSelection::Library, window, cx)
                    }))
                }),
            )
            .into_any_element()
    }

    pub(super) fn download_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let label = self.active_download.as_ref()?;
        if matches!(self.modal, Some(Modal::Downloads)) {
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
        let mut card =
            div()
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
                .child(div().mt_2().text_xs().text_color(rgb(colors.muted)).child(
                    localized_format!(
                        "В ожидании: {} · Открыть загрузки",
                        "Waiting: {} · Open downloads",
                        self.pending_downloads.len()
                    ),
                ))
                .on_click(
                    cx.listener(|app, _, window, cx| app.open_modal(Modal::Downloads, window, cx)),
                );
        if let Some(cancel) = &self.download_cancel {
            let token = cancel.clone();
            card = card.child(
                div()
                    .id("cancel-mini-download")
                    .relative()
                    .mt_2()
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .child(if cancel.load(Ordering::Relaxed) {
                        tr("Останавливаем…")
                    } else {
                        tr("Остановить")
                    })
                    .child(self.button_feedback(4.))
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
                    .relative()
                    .text_xs()
                    .cursor_pointer()
                    .text_color(rgb(colors.muted))
                    .hover(move |s| s.text_color(rgb(colors.text)))
                    .child(if self.error_details_open {
                        tr("Свернуть")
                    } else {
                        tr("Подробнее")
                    })
                    .child(self.button_feedback(4.))
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
        let favorite = self.favorite_ids.contains(&id);
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
            .child(self.track_artwork(&track, 40.))
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
                div()
                    .id(("favorite-track", id as u64))
                    .relative()
                    .size_8()
                    .flex_shrink_0()
                    .rounded_lg()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |style| style.bg(rgb(colors.button_hover)))
                    .child(
                        svg()
                            .path(if favorite {
                                "icons/heart.svg"
                            } else {
                                "icons/heart-outline.svg"
                            })
                            .size_5()
                            .text_color(rgb(if favorite {
                                colors.accent
                            } else {
                                colors.muted
                            })),
                    )
                    .child(self.button_feedback(8.))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.toggle_favorite(id, cx);
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
        let current = state.track_id.and_then(|id| self.find_playback_track(id));
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
                    .child(self.track_artwork(track, 42.))
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
                                if let Some(track) = app.find_playback_track(id) {
                                    row = row
                                        .child(app.track_artwork(track, 32.))
                                        .child(app.track_caption(track, false));
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
        if overlay {
            panel = panel
                .absolute()
                .right_0()
                .top_0()
                .bottom_0()
                .shadow_lg()
                .occlude();
        }
        panel
            .with_animation(
                "queue-enter",
                Animation::new(Duration::from_millis(180)).with_easing(ease_out_quint()),
                |panel, t| panel.opacity(t),
            )
            .into_any_element()
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
            SliderKind::CursorPetSpeed => {
                self.preferences.cursor_pet_speed = 20 + (value * 130.).round() as u16;
                cx.notify();
            }
            SliderKind::BackgroundBlur => {
                self.blur_preview = Some((value * 40.).round() as u8);
                cx.notify();
            }
        }
    }

    fn finish_slider(&mut self, cx: &mut Context<Self>) {
        let Some(kind) = self.dragging_slider.take() else {
            return;
        };
        match Some(kind) {
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
            Some(SliderKind::Volume | SliderKind::CursorPetSpeed) => self.save_preferences(),
            None => {}
        }
        cx.notify();
    }

    fn footer(&self, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let state = self.player.state();
        let current = state.track_id.and_then(|id| self.find_playback_track(id));
        let mut caption = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap_3()
            .children(
                current.map(|track| self.track_artwork(track, if compact { 36. } else { 48. })),
            );
        caption = if let Some(track) = current {
            let id = track.id;
            let favorite = self.favorite_ids.contains(&id);
            caption
                .child(self.track_caption(track, false))
                .child(
                    self.icon_button("ignore-playing-track", "⊘", self.ignored_ids.contains(&id))
                        .when(id < 0, |s| s.invisible())
                        .tooltip(|_, cx| {
                            cx.new(|_| {
                                super::sidebar::SidebarTooltip(tr("Игнорировать трек").into())
                            })
                            .into()
                        })
                        .on_click(cx.listener(move |app, _, _, cx| {
                            if id > 0 {
                                app.set_track_ignored(id, true, cx);
                            }
                        })),
                )
                .child(
                    div()
                        .id("favorite-playing-track")
                        .relative()
                        .size_8()
                        .flex_shrink_0()
                        .rounded_lg()
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(move |s| s.bg(rgb(colors.button_hover)))
                        .child(
                            svg()
                                .path(if favorite {
                                    "icons/heart.svg"
                                } else {
                                    "icons/heart-outline.svg"
                                })
                                .size_5()
                                .text_color(rgb(if favorite {
                                    colors.accent
                                } else {
                                    colors.muted
                                })),
                        )
                        .child(self.button_feedback(8.))
                        .when(id > 0, |s| {
                            s.on_click(
                                cx.listener(move |app, _, _, cx| app.toggle_favorite(id, cx)),
                            )
                        })
                        .when(id < 0, |s| s.invisible()),
                )
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
                    .child(self.playback_mode_control("shuffle-player", 32., cx))
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
                            .relative()
                            .size_10()
                            .rounded_full()
                            .bg(rgb(colors.accent))
                            .text_color(rgb(colors.bg))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_lg()
                            .child(if state.buffering {
                                self.search_spinner("player-buffering", 20.)
                            } else {
                                svg()
                                    .path(if current.is_some() && !state.paused {
                                        "icons/pause.svg"
                                    } else {
                                        "icons/play.svg"
                                    })
                                    .size_5()
                                    .into_any_element()
                            })
                            .child(self.button_feedback(20.))
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
            .child(self.progress_view());
        let volume_tools = div()
            .w(px(if compact { 264. } else { 280. }))
            .max_w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .child(
                self.icon_button("expand-track", "⛶", self.now_playing_open)
                    .tooltip(|_, cx| {
                        cx.new(|_| {
                            super::sidebar::SidebarTooltip(localized_format!(
                                "Полный экран · F11",
                                "Fullscreen · F11"
                            ))
                        })
                        .into()
                    })
                    .on_click(cx.listener(|app, _, window, cx| app.toggle_now_playing(window, cx))),
            )
            .child(
                div()
                    .id("toggle-queue")
                    .relative()
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
                    .child(self.button_feedback(8.))
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
        let active_context = state.track_id.is_some()
            && self.playback_playlist == self.selected_playlist
            && self.playback_context == self.current_title();
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
            .child(self.playback_mode_control("shuffle-collection", 44., cx));
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
                    .child(div().flex_1().min_w_0().child(self.search_controls(cx)))
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
                    .child(div().text_lg().child(if self.library_loading {
                        localized_format!("Загружаем треки…", "Loading tracks…")
                    } else if searching {
                        tr("Ничего не найдено").into()
                    } else {
                        tr("Здесь будет ваша музыка").into()
                    }))
                    .child(div().text_sm().text_color(rgb(colors.muted)).child(
                        if self.library_loading {
                            ""
                        } else if searching {
                            tr("Попробуйте другое название или исполнителя")
                        } else {
                            tr("Добавьте музыку или выберите треки из библиотеки")
                        },
                    )),
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
        self.update_background_animation(window, cx);
        if self.image_cache_dirty {
            self.images.update(cx, |cache, cx| cache.clear(window, cx));
            self.image_cache_dirty = false;
        }
        for path in std::mem::take(&mut self.image_invalidations) {
            self.images
                .update(cx, |cache, cx| cache.remove_path(path, window, cx));
        }
        self.images
            .update(cx, |cache, cx| cache.begin_frame(window, cx));
        if self.storage_moving {
            return self.moving_storage_view(cx);
        }
        if self.storage_clearing {
            return div()
                .size_full()
                .bg(rgb(self.colors().bg))
                .text_color(rgb(self.colors().text))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_4()
                .child(self.brand_mark(56.))
                .child(localized_format!("Очищаем хранилище…", "Clearing storage…"));
        }
        let colors = self.colors();
        let viewport = window.viewport_size();
        let compact = viewport.width < px(1100.);
        self.configure_progress(window, cx);
        self.cursor_companion.update(cx, |view, cx| {
            view.configure(
                self.preferences.cursor_pet,
                self.preferences.cursor_pet_speed,
                window,
                cx,
            )
        });
        self.configure_pet_picker(window, cx);
        if self.library_body.is_none() {
            let parent = cx.weak_entity();
            let body = cx.new(|_| LibraryBody { parent });
            cx.observe(&cx.entity(), {
                let weak = body.downgrade();
                move |_, _, cx| {
                    let _ = weak.update(cx, |_, cx| cx.notify());
                }
            })
            .detach();
            self.library_body = Some(body);
        }
        let body = AnyView::from(self.library_body.as_ref().unwrap().clone())
            .cached(div().flex_1().min_h_0().w_full().style().clone());
        div()
            .relative()
            .size_full()
            .bg(rgb(colors.bg))
            .text_color(rgb(colors.text))
            .font_family("Segoe UI")
            .text_size(px(14.))
            .track_focus(&self.focus)
            .key_context("MusicApp")
            .on_action(cx.listener(|app, _: &FullscreenTrack, window, cx| {
                app.toggle_now_playing(window, cx)
            }))
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
            .on_mouse_move(cx.listener(|app, event: &MouseMoveEvent, window, cx| {
                app.cursor_companion
                    .update(cx, |view, cx| view.follow(event.position, window, cx));
                if let Some(kind) = app.dragging_slider {
                    app.change_slider(kind, event.position, cx);
                }
                if app.progress.read(cx).is_dragging() {
                    app.progress
                        .update(cx, |view, cx| view.drag(event.position, cx));
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _, _, cx| {
                    app.progress.update(cx, |view, cx| view.finish(cx));
                    app.finish_slider(cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|app, _, _, cx| {
                    app.progress.update(cx, |view, cx| view.finish(cx));
                    app.finish_slider(cx);
                }),
            )
            .flex()
            .flex_col()
            .children(self.active_background().map(|background| {
                div()
                    .absolute()
                    .inset_0()
                    .overflow_hidden()
                    .child(self.background_visual(background))
                    .child(div().absolute().inset_0().bg(rgba((colors.bg << 8) | 0x94)))
            }))
            .child(if self.now_playing_open {
                self.now_playing_view(viewport, cx)
            } else {
                div()
                    .size_full()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(body)
                    .child(self.footer(compact, cx))
                    .into_any_element()
            })
            .children(self.floating_update_notice(viewport, cx))
            .children(self.modal_view(viewport, cx))
            .child(self.cursor_companion.clone())
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
        let path = match label {
            "⊘" => Some("icons/ban.svg"),
            "🗑" => Some("icons/trash.svg"),
            "▶" => Some("icons/play.svg"),
            "Ⅱ" => Some("icons/pause.svg"),
            "‹‹" => Some("icons/previous.svg"),
            "››" => Some("icons/next.svg"),
            "×" | "✕" => Some("icons/close.svg"),
            "⋯" => Some("icons/more.svg"),
            "✎" => Some("icons/edit.svg"),
            "⤨" => Some("icons/shuffle.svg"),
            "■" => Some("icons/stop.svg"),
            _ => None,
        };
        let content = if let Some(path) = path {
            svg().path(path).size_5().into_any_element()
        } else {
            div().child(label.to_owned()).into_any_element()
        };
        div()
            .id(id)
            .size_8()
            .relative()
            .flex_shrink_0()
            .rounded_lg()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_size(px(18.))
            .text_color(rgb(if active { colors.accent } else { colors.muted }))
            .bg(if active {
                rgb(colors.active)
            } else {
                rgba(0x00000000)
            })
            .active(|s| s.opacity(0.65))
            .hover(move |s| s.bg(rgb(colors.button_hover)).text_color(rgb(colors.text)))
            .child(content)
            .child(self.button_feedback(8.))
    }

    pub(super) fn track_artwork(&self, track: &Track, size: f32) -> AnyElement {
        if let Some((_, result)) = self.discovery.previews.get(&track.id) {
            return self.search_artwork(result, size);
        }
        let colors = self.colors();
        if !self
            .preview_paths
            .contains(&shard::artwork::track_path(&self.data_directory, track))
        {
            return div()
                .size(px(size))
                .flex_shrink_0()
                .rounded_lg()
                .bg(rgb(colors.button))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path("icons/music.svg")
                        .size(px(size * 0.5))
                        .text_color(rgb(colors.muted)),
                )
                .into_any_element();
        }
        div()
            .size(px(size))
            .flex_shrink_0()
            .rounded_lg()
            .overflow_hidden()
            .child(
                img(shard::artwork::thumbnail_path(&shard::artwork::track_path(
                    &self.data_directory,
                    track,
                )))
                .image_cache(&self.images)
                .size_full()
                .object_fit(ObjectFit::Cover)
                .with_fallback(move || {
                    div()
                        .size_full()
                        .bg(rgb(colors.button))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            svg()
                                .path("icons/music.svg")
                                .size(px(size * 0.5))
                                .text_color(rgb(colors.muted)),
                        )
                        .into_any_element()
                }),
            )
            .into_any_element()
    }

    pub(super) fn track_caption(&self, track: &Track, _active: bool) -> Div {
        let colors = self.colors();
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .w_full()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_color(rgb(colors.text))
                            .child(track.title.clone()),
                    )
                    .children(self.ignored_ids.contains(&track.id).then(|| {
                        div()
                            .flex_shrink_0()
                            .px_2()
                            .rounded_md()
                            .text_size(px(10.))
                            .bg(rgb(colors.button))
                            .text_color(rgb(colors.muted))
                            .child(tr("Игнор"))
                    })),
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

impl MusicApp {
    fn render_library_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        {
            self.library_renders += 1;
        }
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
            .size_full()
            .flex()
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
                    .child(
                        if self.library_loading && self.playlists.is_empty() && !self.settings_open
                        {
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(rgb(colors.muted))
                                .child(localized_format!(
                                    "Загружаем библиотеку…",
                                    "Loading library…"
                                ))
                                .into_any_element()
                        } else if self.settings_open {
                            self.settings_view(cx)
                        } else if self.discovery.online {
                            self.online_search_view(cx)
                        } else if self.home_open && self.search.read(cx).value().is_empty() {
                            self.home_view(viewport, cx)
                        } else {
                            content.into_any_element()
                        },
                    )
                    .children(
                        (!self.home_open && !self.settings_open && !self.discovery.online && {
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
        body.into_any_element()
    }
}

pub(super) struct LibraryBody {
    parent: WeakEntity<MusicApp>,
}
impl Render for LibraryBody {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.parent
            .update(cx, |app, cx| app.render_library_body(window, cx))
            .unwrap_or_else(|_| div().into_any_element())
    }
}
