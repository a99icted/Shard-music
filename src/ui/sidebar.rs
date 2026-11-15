use super::*;

struct SidebarTooltip(String);
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
    fn sidebar_icon(&self, id: impl Into<ElementId>, title: String, active: bool) -> Stateful<Div> {
        let colors = self.colors();
        div()
            .id(id)
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
                .child(img(self.preferences.theme.logo_path()).size_10())
                .on_click(cx.listener(|app, _, window, cx| {
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
                .child("▤")
                .on_click(cx.listener(|app, _, _, cx| {
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
                    .children(self.playlists.iter().map(|playlist| {
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
                                app.open_modal(Modal::PlaylistActions(actions.clone()), window, cx);
                            }),
                        )
                    })),
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
        if self.busy.is_some() {
            view = view.child(
                self.sidebar_icon("compact-download", tr("Добавить музыку").into(), false)
                    .text_xl()
                    .child("↓")
                    .on_click(
                        cx.listener(|app, _, window, cx| app.open_modal(Modal::Import, window, cx)),
                    ),
            );
        }
        view.child(
            self.sidebar_icon("open-settings", tr("Настройки").into(), self.settings_open)
                .text_xl()
                .child("⚙")
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
        let mini_download =
            compact && self.busy.is_some() && !matches!(self.modal, Some(Modal::Import));
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
                    .child(
                        img(self.preferences.theme.logo_path())
                            .size(px(if compact { 44. } else { 52. }))
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .child("Shard"),
                    )
            }))
            .children((!mini_download).then(|| {
                div()
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .mb_2()
                    .child(tr("ВАША МУЗЫКА"))
            }))
            .children(mini_download.then(|| {
                self.button("mini-home", tr("⌂ Главная"), false)
                    .py_2()
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.settings_open = false;
                        app.home_open = true;
                        app.refresh(cx);
                    }))
            }))
            .child(
                div()
                    .id("library")
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
                    .child(div().text_color(rgb(colors.accent)).child("▤"))
                    .child(div().flex_1().child(tr("Все треки")))
                    .child(div().text_xs().child(self.tracks.len().to_string()))
                    .on_click(cx.listener(|app, _, _, cx| {
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
                    .overflow_y_scroll()
                    .children(self.playlists.iter().map(|playlist| {
                        let id = playlist.id;
                        let delete = playlist.clone();
                        div()
                            .id(("playlist", id as u64))
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
                                    .child(self.playlist_artwork(playlist, 24.))
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
                                            .child(playlist.track_count.to_string()),
                                    ),
                            )
                            .on_click(cx.listener(move |app, _, _, cx| {
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
                    })),
            )
            .children(self.sync_progress_card())
            .children(self.download_card(cx))
            .child(
                self.button("open-settings", tr("⚙  Настройки"), self.settings_open)
                    .py_2()
                    .on_click(cx.listener(|app, _, window, cx| {
                        app.settings_open = true;
                        app.modal = None;
                        app.blur_preview = None;
                        window.focus(&app.focus);
                        cx.notify();
                    })),
            )
            .children((!compact).then(|| {
                div()
                    .pt_4()
                    .border_t_1()
                    .border_color(rgb(colors.border))
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .child(tr("●  Локальная коллекция"))
                    .child(div().mt_2().child(tr("Ваша музыка. На вашем устройстве.")))
            }))
            .into_any_element()
    }
}
