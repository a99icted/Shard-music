use super::*;

pub(super) fn listening_time(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    if seconds < 60 {
        localized_format!("{seconds} сек", "{seconds} sec")
    } else if seconds < 3600 {
        localized_format!("{} мин", "{} min", seconds / 60)
    } else {
        localized_format!(
            "{} ч {} мин",
            "{} hr {} min",
            seconds / 3600,
            seconds % 3600 / 60
        )
    }
}

impl MusicApp {
    pub(super) fn refresh_statistics(&mut self, cx: &mut Context<Self>) {
        if self.statistics_loading {
            return;
        }
        self.statistics_loading = true;
        self.statistics_updated = Instant::now();
        let service = self.statistics.clone();
        let task = cx
            .background_executor()
            .spawn(async move { service.summary() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                app.statistics_loading = false;
                let changed;
                match result {
                    Ok(summary) => {
                        changed = app.listening != summary;
                        app.listening = summary;
                    }
                    Err(error) => {
                        changed = true;
                        app.error = Some(localized_format!(
                            "Не удалось прочитать статистику: {error:#}",
                            "Could not read statistics: {error:#}"
                        ))
                    }
                }
                if changed && app.home_open {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn home_view(&self, viewport: Size<Pixels>, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let dense = viewport.width < px(1100.) || viewport.height < px(700.);
        let available = (f32::from(viewport.width)
            - self.sidebar_width(dense)
            - if dense { 32. } else { 64. }
            - if self.preferences.queue_visible && viewport.width >= px(1280.) {
                288.
            } else {
                0.
            }
            - 4.)
            .clamp(220., 1120.);
        let columns = ((available + 12.) / 260.).floor().max(1.) as usize;
        let card_width = ((available - 12. * (columns - 1) as f32) / columns as f32).max(220.);
        let mut view = div()
            .id("home-scroll")
            .size_full()
            .max_w(px(1120.))
            .min_w_0()
            .overflow_y_scroll()
            .py(px(if dense { 16. } else { 28. }))
            .flex()
            .flex_col()
            .gap(px(if dense { 20. } else { 28. }))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .child(tr("Главная")),
                    )
                    .child(self.button("home-import", tr("+ Музыка"), true).on_click(
                        cx.listener(|app, _, window, cx| app.open_modal(Modal::Import, window, cx)),
                    )),
            )
            .children(self.notifications(cx));

        // The three most played tracks are the first content on the page.
        if !self.listening.top.is_empty() {
            let mut podium = div().flex().items_end().gap_3();
            for (rank, track) in self.listening.top.iter().enumerate() {
                let id = track.id;
                let tint = [0xe6bf75, 0xb6c5dc, 0xc79176][rank.min(2)];
                podium = podium.child(
                    div()
                        .id(("top-track", id as u64))
                        .flex_1()
                        .min_w_0()
                        .h(px(if rank == 0 { 132. } else { 122. }))
                        .p_3()
                        .rounded_2xl()
                        .border_1()
                        .border_color(rgba((tint << 8) | 0x35))
                        .overflow_hidden()
                        .cursor_pointer()
                        .bg(linear_gradient(
                            125.,
                            linear_color_stop(rgba((tint << 8) | 0x28), 0.),
                            linear_color_stop(self.surface(colors.panel), 1.),
                        ))
                        .hover(move |s| s.border_color(rgba((tint << 8) | 0x90)))
                        .active(|s| s.opacity(0.75))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(
                                    div()
                                        .text_size(px(if rank == 0 { 24. } else { 20. }))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(rgb(tint))
                                        .child(format!("{:02}", rank + 1)),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(colors.muted))
                                        .child(format!("{} ×", track.plays)),
                                ),
                        )
                        .child(
                            div()
                                .mt_3()
                                .flex()
                                .items_center()
                                .gap_2()
                                .children(self.tracks.iter().find(|t| t.id == id).map(|track| {
                                    self.track_artwork(track, if dense { 32. } else { 44. })
                                }))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .child(track.title.clone()),
                                        )
                                        .child(
                                            div()
                                                .mt_1()
                                                .text_xs()
                                                .text_color(rgb(colors.muted))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .child(track.artist.clone()),
                                        ),
                                ),
                        )
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.settings_open = false;
                            app.home_open = false;
                            app.selected_playlist = None;
                            app.search.update(cx, |input, cx| {
                                input.reset();
                                cx.notify();
                            });
                            app.refresh(cx);
                            if let Some(track) = app.tracks.iter().find(|t| t.id == id).cloned() {
                                app.play(track, cx);
                            }
                        }))
                        .with_animation(
                            ("podium-enter", id as u64),
                            Animation::new(Duration::from_millis(220 + rank as u64 * 45))
                                .with_easing(ease_out_quint()),
                            |card, t| card.relative().top(px(8. * (1. - t))).opacity(t),
                        ),
                );
            }
            view = view.child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(localized_format!("На повторе", "On repeat")),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(colors.muted))
                                    .child(tr("Топ-3 за всё время")),
                            ),
                    )
                    .child(podium),
            );
        }

        view = view.child(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(tr("Ваши плейлисты")),
                )
                .child(
                    self.button("home-new-playlist", tr("+ Создать"), false)
                        .py_2()
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.open_modal(Modal::CreatePlaylist, window, cx)
                        })),
                ),
        );
        let limit = if self.home_playlists_expanded {
            self.playlists.len()
        } else {
            columns * 2
        };
        let mut cards = div().flex_shrink_0().flex().flex_wrap().gap_3();
        for playlist in self.playlists.iter().take(limit) {
            let id = playlist.id;
            cards = cards.child(
                div()
                    .id(("home-playlist", id as u64))
                    .w(px(card_width))
                    .flex_shrink_0()
                    .p_3()
                    .rounded_xl()
                    .bg(self.surface(colors.panel))
                    .border_1()
                    .border_color(rgb(colors.border))
                    .hover(move |s| {
                        s.border_color(rgb(colors.accent))
                            .bg(rgb(colors.button_hover))
                    })
                    .active(|s| s.opacity(0.8))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.playlist_artwork(playlist, 48.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(playlist.name.clone()),
                            )
                            .child(div().mt_2().text_xs().text_color(rgb(colors.muted)).child(
                                localized_format!("{} треков", "{} tracks", playlist.track_count),
                            )),
                    )
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.settings_open = false;
                        app.home_open = false;
                        app.selected_playlist = Some(id);
                        app.search.update(cx, |input, cx| {
                            input.reset();
                            cx.notify();
                        });
                        app.refresh(cx);
                        window.focus(&app.focus);
                    }))
                    .with_animation(
                        ("playlist-enter", id as u64),
                        Animation::new(Duration::from_millis(150)).with_easing(ease_out_quint()),
                        |card, t| card.relative().top(px(4. * (1. - t))).opacity(t),
                    ),
            );
        }
        view = view.child(cards);
        if self.playlists.len() > columns * 2 {
            view = view.child(
                self.button(
                    "home-expand-playlists",
                    &if self.home_playlists_expanded {
                        localized_format!("Свернуть", "Show less")
                    } else {
                        localized_format!(
                            "Все плейлисты · {}",
                            "All playlists · {}",
                            self.playlists.len()
                        )
                    },
                    false,
                )
                .py_2()
                .on_click(cx.listener(|app, _, _, cx| {
                    app.home_playlists_expanded = !app.home_playlists_expanded;
                    cx.notify();
                })),
            );
        }

        // Compact activity strip; the weekly chart is optional instead of taking the first screen.
        let mut activity = div()
            .flex_shrink_0()
            .p_4()
            .rounded_xl()
            .bg(self.surface(colors.panel))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div().flex_1().min_w_0().flex().gap_4().children(
                            [
                                (tr("Сегодня"), self.listening.today_ms),
                                (tr("В этом месяце"), self.listening.month_ms),
                                (tr("В этом году"), self.listening.year_ms),
                            ]
                            .into_iter()
                            .map(|(label, ms)| {
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div().text_xs().text_color(rgb(colors.muted)).child(label),
                                    )
                                    .child(
                                        div()
                                            .mt_2()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(listening_time(ms)),
                                    )
                            }),
                        ),
                    )
                    .child(
                        self.icon_button(
                            "home-activity",
                            if self.home_activity_expanded {
                                "−"
                            } else {
                                "+"
                            },
                            self.home_activity_expanded,
                        )
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.home_activity_expanded = !app.home_activity_expanded;
                            cx.notify();
                        })),
                    ),
            );
        if self.home_activity_expanded {
            let maximum = self
                .listening
                .week
                .iter()
                .map(|(_, ms)| *ms)
                .max()
                .unwrap_or(1)
                .max(1);
            activity = activity.child(
                div()
                    .mt_4()
                    .flex()
                    .items_end()
                    .gap_2()
                    .h(px(86.))
                    .children(self.listening.week.iter().map(|(day, ms)| {
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .justify_end()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(48.))
                                    .h(px(4. + 48. * *ms as f32 / maximum as f32))
                                    .rounded_lg()
                                    .bg(rgb(if *ms > 0 {
                                        colors.accent
                                    } else {
                                        colors.border
                                    })),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(colors.muted))
                                    .child(day.clone()),
                            )
                    }))
                    .with_animation(
                        "weekly-enter",
                        Animation::new(Duration::from_millis(180)),
                        |graph, t| graph.opacity(t),
                    ),
            );
        }
        view.child(activity)
            .with_animation(
                "home-enter",
                Animation::new(Duration::from_millis(180)),
                |view, t| view.opacity(t),
            )
            .into_any_element()
    }
}
