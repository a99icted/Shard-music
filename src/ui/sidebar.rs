use super::*;

pub(super) struct SidebarTooltip(pub(super) String);
impl Render for SidebarTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.global::<UiTheme>().0.palette();
        div()
            .px_3()
            .py_2()
            .rounded_lg()
            .bg(rgb(colors.panel))
            .border_1()
            .border_color(rgb(colors.border))
            .text_sm()
            .text_color(rgb(colors.text))
            .shadow_lg()
            .child(self.0.clone())
    }
}

impl MusicApp {
    fn favorites_navigation(&self, collapsed: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let active = !self.home_open
            && !self.settings_open
            && self.selected_playlist == Some(shard::favorites::FAVORITES_ID);
        let icon = svg()
            .path("icons/heart.svg")
            .size_5()
            .text_color(rgb(colors.accent));
        let mut item = if collapsed {
            self.sidebar_icon("favorites", tr("Избранное").into(), active)
                .child(icon)
        } else {
            div()
                .id("favorites")
                .relative()
                .flex_shrink_0()
                .px_3()
                .py_3()
                .rounded_lg()
                .cursor_pointer()
                .bg(rgb(if active { colors.active } else { colors.panel }))
                .text_color(rgb(if active { colors.accent } else { colors.text }))
                .hover(move |s| s.bg(rgb(colors.button_hover)))
                .flex()
                .items_center()
                .gap_2()
                .child(icon)
                .child(div().flex_1().child(tr("Избранное")))
                .child(div().text_xs().child(self.favorite_ids.len().to_string()))
                .child(self.button_feedback(8.))
        };
        item = item.on_click(cx.listener(|app, _, _, cx| {
            app.set_search_mode(false, cx);
            app.settings_open = false;
            app.home_open = false;
            app.modal = None;
            app.selected_playlist = Some(shard::favorites::FAVORITES_ID);
            app.search.update(cx, |search, cx| {
                search.reset();
                cx.notify();
            });
            app.refresh(cx);
        }));
        if let Some(favorites) = self
            .playlists
            .iter()
            .find(|p| p.id == shard::favorites::FAVORITES_ID)
        {
            let actions = favorites.clone();
            item = item.on_mouse_down(
                MouseButton::Right,
                cx.listener(move |app, _, window, cx| {
                    cx.stop_propagation();
                    app.open_modal(Modal::PlaylistActions(actions.clone()), window, cx);
                }),
            );
        }
        item.into_any_element()
    }

    fn sidebar_icon(&self, id: impl Into<ElementId>, title: String, active: bool) -> Stateful<Div> {
        let colors = self.colors();
        div()
            .id(id)
            .relative()
            .w_full()
            .h(px(48.))
            .flex_shrink_0()
            .rounded_lg()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .bg(rgb(if active { colors.active } else { colors.panel }))
            .text_color(rgb(if active { colors.accent } else { colors.muted }))
            .hover(move |s| {
                s.bg(rgb(colors.button_hover))
                    .text_color(rgb(colors.accent))
            })
            .tooltip(move |_, cx| cx.new(|_| SidebarTooltip(title.clone())).into())
            .child(self.button_feedback(8.))
    }

    fn collapsed_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let mut view = div()
            .w(px(72.))
            .h_full()
            .flex_shrink_0()
            .p_2()
            .bg(self.surface(colors.panel))
            .border_r_1()
            .border_color(rgb(colors.border))
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .child(self.sidebar_toggle(cx))
            .child(
                self.sidebar_icon(
                    "home-logo",
                    tr("Главная").into(),
                    self.home_open && !self.settings_open,
                )
                .child(self.brand_mark(36.))
                .on_click(cx.listener(|app, _, window, cx| {
                    app.set_search_mode(false, cx);
                    app.settings_open = false;
                    app.home_open = true;
                    app.modal = None;
                    app.refresh(cx);
                    window.focus(&app.focus);
                })),
            )
            .child(
                self.sidebar_icon(
                    "library",
                    tr("Все треки").into(),
                    !self.home_open && !self.settings_open && self.selected_playlist.is_none(),
                )
                .text_xl()
                .child(svg().path("icons/library.svg").size_5())
                .on_click(cx.listener(|app, _, _, cx| {
                    app.set_search_mode(false, cx);
                    app.settings_open = false;
                    app.home_open = false;
                    app.selected_playlist = None;
                    app.search.update(cx, |s, cx| {
                        s.reset();
                        cx.notify();
                    });
                    app.refresh(cx);
                })),
            )
            .child(self.favorites_navigation(true, cx))
            .child(
                self.sidebar_icon("new-playlist", tr("Создать плейлист").into(), false)
                    .text_2xl()
                    .child("+")
                    .on_click(cx.listener(|app, _, window, cx| {
                        app.open_modal(Modal::CreatePlaylist, window, cx)
                    })),
            )
            .child(
                div()
                    .id("playlist-scroll")
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(
                        self.playlists
                            .iter()
                            .filter(|p| p.id != shard::favorites::FAVORITES_ID)
                            .map(|playlist| {
                                let id = playlist.id;
                                let actions = playlist.clone();
                                self.sidebar_icon(
                                    ("playlist", id as u64),
                                    playlist.name.clone(),
                                    !self.settings_open
                                        && !self.home_open
                                        && self.selected_playlist == Some(id),
                                )
                                .mb_1()
                                .child(self.playlist_artwork(playlist, 36.))
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.set_search_mode(false, cx);
                                    app.settings_open = false;
                                    app.home_open = false;
                                    app.selected_playlist = Some(id);
                                    app.search.update(cx, |s, cx| {
                                        s.reset();
                                        cx.notify();
                                    });
                                    app.refresh(cx);
                                }))
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |app, _, window, cx| {
                                        cx.stop_propagation();
                                        app.open_modal(
                                            Modal::PlaylistActions(actions.clone()),
                                            window,
                                            cx,
                                        );
                                    }),
                                )
                            }),
                    ),
            );
        if let Some(progress) = &self.sync_status.progress {
            let fraction = if progress.complete {
                1.
            } else if progress.total > 0 {
                progress.bytes as f32 / progress.total as f32
            } else {
                0.
            };
            view = view.child(
                self.sidebar_icon("compact-sync-progress", progress.title.clone(), false)
                    .flex_col()
                    .gap_2()
                    .text_xs()
                    .child("⇄")
                    .child(
                        div()
                            .w_8()
                            .h(px(3.))
                            .rounded_full()
                            .overflow_hidden()
                            .bg(rgb(colors.border))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(fraction.clamp(0., 1.)))
                                    .bg(rgb(colors.accent)),
                            ),
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.settings_open = true;
                        app.settings_category = settings::SettingsCategory::Synchronization;
                        cx.notify();
                    })),
            );
        }
        if self.active_download.is_some() {
            view = view.child(
                self.sidebar_icon("compact-download", tr("Загрузки").into(), false)
                    .text_xl()
                    .child(svg().path("icons/download.svg").size_5())
                    .on_click(cx.listener(|app, _, window, cx| {
                        app.open_modal(Modal::Downloads, window, cx)
                    })),
            );
        }
        view.child(
            self.sidebar_icon("open-settings", tr("Настройки").into(), self.settings_open)
                .text_xl()
                .child(svg().path("icons/settings.svg").size_5())
                .on_click(cx.listener(|app, _, window, cx| {
                    app.settings_open = true;
                    app.modal = None;
                    app.blur_preview = None;
                    window.focus(&app.focus);
                    cx.notify();
                })),
        )
        .into_any_element()
    }

    pub(super) fn sidebar_width(&self, compact: bool) -> f32 {
        if self.preferences.sidebar_collapsed {
            72.
        } else if compact {
            176.
        } else {
            224.
        }
    }

    fn sidebar_toggle(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let colors = self.colors();
        div()
            .id("toggle-sidebar")
            .relative()
            .size_8()
            .rounded_lg()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(move |s| s.bg(rgb(colors.button_hover)))
            .child(
                svg()
                    .path(if self.preferences.sidebar_collapsed {
                        "icons/panel-expand.svg"
                    } else {
                        "icons/panel-collapse.svg"
                    })
                    .size_5()
                    .text_color(rgb(colors.muted)),
            )
            .tooltip(|_, cx| {
                cx.new(|_| SidebarTooltip(tr("Свернуть / развернуть боковую панель").into()))
                    .into()
            })
            .child(self.button_feedback(8.))
            .on_click(cx.listener(|app, _, _, cx| {
                app.preferences.sidebar_collapsed = !app.preferences.sidebar_collapsed;
                app.save_preferences();
                cx.notify();
            }))
    }

    pub(super) fn sidebar(&self, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        if self.preferences.sidebar_collapsed {
            return self.collapsed_sidebar(cx);
        }
        let colors = self.colors();
        let selected = self.selected_playlist;
        let playlist_indices: Vec<_> = self
            .playlists
            .iter()
            .enumerate()
            .filter_map(|(index, playlist)| {
                (playlist.id != shard::favorites::FAVORITES_ID).then_some(index)
            })
            .collect();
        let mini_download =
            compact && self.active_download.is_some() && !matches!(self.modal, Some(Modal::Import));
        div()
            .w(px(self.sidebar_width(compact)))
            .h_full()
            .flex_shrink_0()
            .bg(self.surface(colors.panel))
            .border_r_1()
            .border_color(rgb(colors.border))
            .p(px(if compact { 12. } else { 20. }))
            .flex()
            .flex_col()
            .gap(px(if mini_download { 4. } else { 8. }))
            .child(div().flex().justify_end().child(self.sidebar_toggle(cx)))
            .children((!mini_download).then(|| {
                div()
                    .id("home-logo")
                    .cursor_pointer()
                    .on_click(cx.listener(|app, _, window, cx| {
                        app.set_search_mode(false, cx);
                        app.settings_open = false;
                        app.home_open = true;
                        app.modal = None;
                        app.search.update(cx, |search, cx| {
                            search.reset();
                            cx.notify();
                        });
                        app.refresh(cx);
                        window.focus(&app.focus);
                    }))
                    .flex()
                    .items_center()
                    .gap_3()
                    .mb(px(if compact { 12. } else { 28. }))
                    .mt_2()
                    .child(self.brand_mark(if compact { 36. } else { 42. }))
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(if compact { 23. } else { 27. }))
                            .child(shard::app_identity::HEADER),
                    )
            }))
            .children(mini_download.then(|| {
                self.button("mini-home", tr("⌂ Главная"), false)
                    .py_2()
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.set_search_mode(false, cx);
                        app.settings_open = false;
                        app.home_open = true;
                        app.refresh(cx);
                    }))
            }))
            .child(
                div()
                    .id("library")
                    .relative()
                    .flex_shrink_0()
                    .px_3()
                    .py_3()
                    .rounded_lg()
                    .cursor_pointer()
                    .bg(rgb(if selected.is_none() {
                        colors.active
                    } else {
                        colors.panel
                    }))
                    .text_color(rgb(if selected.is_none() {
                        colors.accent
                    } else {
                        colors.text
                    }))
                    .hover(move |s| s.bg(rgb(colors.button_hover)))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        svg()
                            .path("icons/library.svg")
                            .size_5()
                            .text_color(rgb(colors.accent)),
                    )
                    .child(div().flex_1().child(tr("Все треки")))
                    .child(div().text_xs().child(self.tracks.len().to_string()))
                    .child(self.button_feedback(8.))
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.set_search_mode(false, cx);
                        app.settings_open = false;
                        app.home_open = false;
                        app.selected_playlist = None;
                        app.search.update(cx, |s, cx| {
                            s.reset();
                            cx.notify();
                        });
                        app.refresh(cx);
                    })),
            )
            .child(self.favorites_navigation(false, cx))
            .child(
                div()
                    .mt(px(if compact { 12. } else { 28. }))
                    .mb_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors.muted))
                            .child(tr("ПЛЕЙЛИСТЫ")),
                    )
                    .child(
                        div()
                            .id("new-playlist")
                            .cursor_pointer()
                            .text_lg()
                            .text_color(rgb(colors.muted))
                            .hover(move |s| s.text_color(rgb(colors.accent)))
                            .child("+")
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.open_modal(Modal::CreatePlaylist, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .id("playlist-scroll")
                    .flex_1()
                    .min_h(px(if mini_download { 48. } else { 0. }))
                    .overflow_hidden()
                    .child(
                        uniform_list(
                            "sidebar-playlists",
                            playlist_indices.len(),
                            cx.processor(move |app, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .map(|index| {
                                        let playlist = &app.playlists[playlist_indices[index]];
                                        let id = playlist.id;
                                        let delete = playlist.clone();
                                        div()
                                            .id(("playlist", id as u64))
                                            .relative()
                                            .px_3()
                                            .py_3()
                                            .mb_1()
                                            .rounded_lg()
                                            .cursor_pointer()
                                            .bg(rgb(if selected == Some(id) {
                                                colors.active
                                            } else {
                                                colors.panel
                                            }))
                                            .hover(move |s| s.bg(rgb(colors.button_hover)))
                                            .child(
                                                div()
                                                    .flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(app.playlist_artwork(playlist, 24.))
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .overflow_hidden()
                                                            .text_ellipsis()
                                                            .child(playlist.name.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(rgb(colors.muted))
                                                            .child(
                                                                playlist.track_count.to_string(),
                                                            ),
                                                    ),
                                            )
                                            .child(app.button_feedback(8.))
                                            .on_click(cx.listener(move |app, _, _, cx| {
                                                app.set_search_mode(false, cx);
                                                app.settings_open = false;
                                                app.home_open = false;
                                                app.selected_playlist = Some(id);
                                                app.search.update(cx, |s, cx| {
                                                    s.reset();
                                                    cx.notify();
                                                });
                                                app.refresh(cx);
                                            }))
                                            .on_mouse_down(
                                                MouseButton::Right,
                                                cx.listener(move |app, _, window, cx| {
                                                    cx.stop_propagation();
                                                    app.open_modal(
                                                        Modal::PlaylistActions(delete.clone()),
                                                        window,
                                                        cx,
                                                    );
                                                }),
                                            )
                                    })
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .size_full(),
                    ),
            )
            .children(self.sync_progress_card())
            .children(self.download_card(cx))
            .child(
                self.button("open-settings", "", self.settings_open)
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(svg().path("icons/settings.svg").size_5())
                    .child(tr("Настройки"))
                    .py_2()
                    .on_click(cx.listener(|app, _, window, cx| {
                        app.settings_open = true;
                        app.modal = None;
                        app.blur_preview = None;
                        window.focus(&app.focus);
                        cx.notify();
                    })),
            )
            .children(if compact {
                None
            } else {
                self.update_notice(cx)
            })
            .into_any_element()
    }
}
