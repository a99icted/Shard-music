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
                match result {
                    Ok(summary) => app.listening = summary,
                    Err(error) => {
                        app.error = Some(localized_format!(
                            "Не удалось прочитать статистику: {error:#}",
                            "Could not read statistics: {error:#}"
                        ))
                    }
                }
                if app.home_open {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn home_view(&self, viewport: Size<Pixels>, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let dense = viewport.width < px(1100.) || viewport.height < px(700.);
        let available = f32::from(viewport.width)
            - if dense { 208. } else { 288. }
            - if self.preferences.queue_visible && viewport.width >= px(1280.) {
                288.
            } else {
                0.
            };
        // Subtract a few pixels for scroll gutter and fractional layout rounding.
        let available = (available - 4.).max(220.);
        let columns = ((available.min(1120.) + 12.) / 260.).floor().max(1.) as usize;
        let card_width =
            ((available.min(1120.) - 12. * (columns - 1) as f32) / columns as f32).max(220.);
        let mut view = div()
            .id("home-scroll")
            .size_full()
            .max_w(px(1120.))
            .min_w_0()
            .overflow_y_scroll()
            .py(px(if dense { 12. } else { 28. }))
            .flex()
            .flex_col()
            .gap_4()
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
                            .child(tr("Ваша музыка")),
                    )
                    .child(self.button("home-import", tr("+ Музыка"), true).on_click(
                        cx.listener(|app, _, window, cx| app.open_modal(Modal::Import, window, cx)),
                    )),
            )
            .children(self.notifications(cx))
            .child(
                div().flex_shrink_0().flex().gap_3().children(
                    [
                        (tr("В этом месяце"), self.listening.month_ms),
                        (tr("В этом году"), self.listening.year_ms),
                    ]
                    .into_iter()
                    .map(|(label, ms)| {
                        div()
                            .flex_1()
                            .min_w_0()
                            .p_4()
                            .rounded_xl()
                            .bg(self.surface(colors.panel))
                            .border_1()
                            .border_color(rgb(colors.border))
                            .child(div().text_xs().text_color(rgb(colors.muted)).child(label))
                            .child(
                                div()
                                    .mt_2()
                                    .text_2xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(listening_time(ms)),
                            )
                    }),
                ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr("Ваши плейлисты")),
                    )
                    .child(
                        self.button("home-new-playlist", tr("+ Создать"), false)
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.open_modal(Modal::CreatePlaylist, window, cx)
                            })),
                    ),
            );
        let mut cards = div().flex_shrink_0().flex().flex_wrap().gap_3();
        for playlist in &self.playlists {
            let id = playlist.id;
            cards = cards.child(
                div()
                    .id(("home-playlist", id as u64))
                    .w(px(card_width))
                    .flex_shrink_0()
                    .p_4()
                    .rounded_xl()
                    .bg(self.surface(colors.panel))
                    .border_1()
                    .border_color(rgb(colors.border))
                    .cursor_pointer()
                    .hover(move |s| s.border_color(rgb(colors.hover_border)))
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .items_center()
                            .child(self.playlist_artwork(playlist, if dense { 64. } else { 80. }))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(playlist.name.clone()),
                                    )
                                    .child(div().text_xs().text_color(rgb(colors.muted)).child(
                                        localized_format!(
                                            "Треков: {}",
                                            "Tracks: {}",
                                            playlist.track_count
                                        ),
                                    ))
                                    .child(div().text_xs().text_color(rgb(colors.muted)).child(
                                        format!(
                                            "Длительность: {}",
                                            listening_time(playlist.duration_ms)
                                        ),
                                    )),
                            ),
                    )
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.settings_open = false;
                        app.home_open = false;
                        app.selected_playlist = Some(id);
                        app.search.update(cx, |search, cx| {
                            search.reset();
                            cx.notify();
                        });
                        app.refresh(cx);
                        window.focus(&app.focus);
                    })),
            );
        }
        view = if self.playlists.is_empty() {
            view.child(
                div()
                    .flex_shrink_0()
                    .p_4()
                    .rounded_lg()
                    .bg(self.surface(colors.panel))
                    .text_color(rgb(colors.muted))
                    .child(tr(
                        "Создайте плейлист или добавьте его по ссылке YouTube или Spotify.",
                    )),
            )
        } else {
            view.child(cards)
        };
        view = view.child(
            div()
                .flex_shrink_0()
                .mt_2()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(tr("Топ-3 за всё время")),
        );
        if self.listening.top.is_empty() {
            view = view.child(
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(rgb(colors.muted))
                    .child(tr("Здесь появятся треки, которые вы слушаете чаще всего.")),
            );
        } else {
            for (index, track) in self.listening.top.iter().enumerate() {
                let id = track.id;
                view = view.child(
                    div()
                        .id(("top-track", id as u64))
                        .flex_shrink_0()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(colors.border))
                        .cursor_pointer()
                        .hover(move |s| s.border_color(rgb(colors.hover_border)))
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .text_color(rgb(colors.accent))
                                .child(format!("{:02}", index + 1)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(track.title.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(colors.muted))
                                        .child(track.artist.clone()),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(colors.muted))
                                .child(localized_format!("{} прослуш.", "{} plays", track.plays)),
                        )
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.settings_open = false;
                            app.home_open = false;
                            app.selected_playlist = None;
                            app.search.update(cx, |search, cx| {
                                search.reset();
                                cx.notify();
                            });
                            app.refresh(cx);
                            if let Some(track) = app.tracks.iter().find(|t| t.id == id).cloned() {
                                app.play(track, cx);
                            }
                        })),
                );
            }
        }
        view.child(div().flex_shrink_0().text_xs().text_color(rgb(colors.muted)).child(tr("Статистика считается с этой версии по времени воспроизведения. Пауза и перемотка не учитываются."))).into_any_element()
    }
}
