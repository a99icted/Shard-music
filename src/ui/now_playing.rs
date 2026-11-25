use super::*;

impl MusicApp {
    pub(super) fn toggle_now_playing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.now_playing_open {
            self.now_playing_open = false;
            if self.restore_fullscreen {
                window.toggle_fullscreen();
            }
            self.restore_fullscreen = false;
        } else {
            self.now_playing_open = true;
            self.restore_fullscreen = !window.is_fullscreen();
            if self.restore_fullscreen {
                window.toggle_fullscreen();
            }
        }
        cx.notify();
    }
    pub(super) fn now_playing_view(
        &self,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = self.colors();
        let state = self.player.state();
        let track = state
            .track_id
            .and_then(|id| self.track_index.get(&id))
            .and_then(|i| self.tracks.get(*i));
        let cover = track
            .map(|t| shard::artwork::track_path(&self.data_directory, t))
            .filter(|p| self.preview_paths.contains(p));
        let size = ((viewport.height - px(320.)) / px(1.))
            .clamp(80., 480.)
            .min(((viewport.width - px(64.)) / px(1.)).max(0.));
        let mut view = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .relative()
                    .h(px(64.))
                    .flex_shrink_0()
                    .px_6()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(self.brand_mark(28.))
                            .child(shard::app_identity::HEADER),
                    )
                    .child(self.icon_button("close-now-playing", "×", false).on_click(
                        cx.listener(|app, _, window, cx| app.toggle_now_playing(window, cx)),
                    )),
            );
        let mut center = div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .px_6();
        if let Some(cover) = cover {
            center = center.child(
                img(cover)
                    .image_cache(&self.images)
                    .size(px(size))
                    .rounded_xl()
                    .object_fit(ObjectFit::Contain)
                    .with_fallback(|| div().into_any_element()),
            );
        }
        center = center
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_center()
                    .child(
                        track
                            .map(|t| t.title.clone())
                            .unwrap_or_else(|| tr("Выберите трек в библиотеке").into()),
                    ),
            )
            .children(track.map(|t| {
                div()
                    .text_lg()
                    .text_color(rgb(colors.muted))
                    .text_center()
                    .child(t.artist.clone())
            }));
        let mut controls = div()
            .relative()
            .w_full()
            .max_w(px(900.))
            .px_6()
            .pb_6()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.progress_view());
        let mut buttons = div().flex().items_center().justify_center().gap_5();
        if let Some(track) = track {
            let id = track.id;
            buttons = buttons.child(
                self.icon_button("ignore-fullscreen", "⊘", self.ignored_ids.contains(&id))
                    .tooltip(|_, cx| {
                        cx.new(|_| super::sidebar::SidebarTooltip(tr("Игнорировать трек").into()))
                            .into()
                    })
                    .on_click(
                        cx.listener(move |app, _, _, cx| app.set_track_ignored(id, true, cx)),
                    ),
            );
        }
        buttons = buttons
            .child(
                self.icon_button("previous-fullscreen", "‹‹", false)
                    .size_12()
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.player.previous();
                        cx.notify();
                    })),
            )
            .child(
                self.icon_button(
                    "play-fullscreen",
                    if state.paused || track.is_none() {
                        "▶"
                    } else {
                        "Ⅱ"
                    },
                    true,
                )
                .size(px(56.))
                .rounded_full()
                .bg(rgb(colors.accent))
                .text_color(rgb(colors.bg))
                .on_click(cx.listener(|app, _, _, cx| app.toggle_playback(cx))),
            )
            .child(
                self.icon_button("next-fullscreen", "››", false)
                    .size_12()
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.player.next();
                        cx.notify();
                    })),
            );
        if let Some(track) = track {
            let id = track.id;
            let favorite = self.favorite_ids.contains(&id);
            buttons = buttons.child(
                div()
                    .id("favorite-fullscreen")
                    .size_10()
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |s| s.bg(rgb(colors.button_hover)))
                    .child(
                        svg()
                            .path(if favorite {
                                "icons/heart.svg"
                            } else {
                                "icons/heart-outline.svg"
                            })
                            .size_6()
                            .text_color(rgb(if favorite {
                                colors.accent
                            } else {
                                colors.muted
                            })),
                    )
                    .on_click(cx.listener(move |app, _, _, cx| app.toggle_favorite(id, cx))),
            );
        }
        controls = controls.child(buttons);
        view = view
            .child(center)
            .child(div().relative().flex().justify_center().child(controls));
        view.into_any_element()
    }
}
