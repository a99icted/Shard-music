use super::*;

impl MusicApp {
    pub(super) fn set_track_ignored(&mut self, id: i64, ignored: bool, cx: &mut Context<Self>) {
        let service = self.ignored_service.clone();
        let was_ignored = self.ignored_ids.contains(&id);
        self.run_job(
            tr("Сохраняем игнорирование…"),
            move || service.set(id, ignored),
            move |app, _, _| {
                if ignored {
                    app.ignored_ids.insert(id);
                    app.favorite_ids.remove(&id);
                } else {
                    app.ignored_ids.remove(&id);
                }
                app.player.set_ignored(app.ignored_ids.clone());
                if ignored && was_ignored && app.player.state().track_id == Some(id) {
                    app.player.next();
                }
                app.message = Some(
                    if ignored {
                        tr("Трек добавлен в игнор")
                    } else {
                        tr("Трек убран из игнора")
                    }
                    .into(),
                );
            },
            cx,
        );
    }

    pub(super) fn delete_ignored_tracks(&mut self, tracks: Vec<Track>, cx: &mut Context<Self>) {
        if self.busy.is_some() || self.storage_clearing || self.storage_moving {
            return;
        }
        let service = self.tracks_service.clone();
        let player = self.player.clone();
        self.modal = None;
        self.run_job(
            tr("Удаляем игнорируемые треки…"),
            move || {
                let count = tracks.len();
                for track in tracks {
                    player.remove_and_wait(track.id)?;
                    service.delete_track(track.id)?;
                }
                Ok(count)
            },
            |app, count, _| {
                app.message = Some(localized_format!(
                    "Удалено игнорируемых треков: {count}",
                    "Ignored tracks deleted: {count}"
                ));
            },
            cx,
        );
    }

    pub(super) fn ignored_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let tracks: Vec<_> = self
            .tracks
            .iter()
            .filter(|track| self.ignored_ids.contains(&track.id))
            .cloned()
            .collect();
        let mut section = div().flex_shrink_0().flex().flex_col().gap_3().p_4()
            .rounded_xl().bg(self.surface(colors.panel)).border_1().border_color(rgb(colors.border))
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD)
                .child(localized_format!("Игнорирование · {}", "Ignored tracks · {}", tracks.len())))
            .child(div().text_sm().text_color(rgb(colors.muted))
                .child(tr("Эти треки остаются в библиотеке и плейлистах, но не добавляются в очередь. Их можно слушать, запустив напрямую.")));
        if tracks.is_empty() {
            section = section.child(div().py_4().text_sm().text_color(rgb(colors.muted)).child(
                tr("Нет игнорируемых треков. Добавьте трек через меню ⋯ → Игнорировать трек."),
            ));
        } else {
            let delete = tracks.clone();
            let count = tracks.len();
            section = section
                .child(
                    self.button(
                        "delete-all-ignored",
                        tr("Удалить все игнорируемые треки…"),
                        false,
                    )
                    .text_color(rgb(0xf29a9a))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.open_modal(Modal::DeleteIgnoredTracks(delete.clone()), window, cx);
                    })),
                )
                .child(
                    uniform_list(
                        "ignored-tracks-list",
                        count,
                        cx.processor(move |app, range: std::ops::Range<usize>, _, cx| {
                            let state = app.player.state();
                            range
                                .map(|index| {
                                    let track = &tracks[index];
                                    let id = track.id;
                                    let play = track.clone();
                                    let delete = track.clone();
                                    let active = state.track_id == Some(id) && !state.paused;
                                    div()
                                        .id(("ignored-track", id as u64))
                                        .h(px(64.))
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(app.track_artwork(track, 40.))
                                        .child(app.track_caption(track, false))
                                        .child(
                                            app.icon_button(
                                                ("play-ignored", id as u64),
                                                if active { "Ⅱ" } else { "▶" },
                                                active,
                                            )
                                            .tooltip(|_, cx| {
                                                cx.new(|_| {
                                                    super::sidebar::SidebarTooltip(
                                                        tr("Послушать трек").into(),
                                                    )
                                                })
                                                .into()
                                            })
                                            .on_click(
                                                cx.listener(move |app, _, _, cx| {
                                                    app.play(play.clone(), cx)
                                                }),
                                            ),
                                        )
                                        .child(
                                            app.icon_button(
                                                ("unignore-track", id as u64),
                                                "×",
                                                false,
                                            )
                                            .tooltip(|_, cx| {
                                                cx.new(|_| {
                                                    super::sidebar::SidebarTooltip(
                                                        tr("Убрать из игнора").into(),
                                                    )
                                                })
                                                .into()
                                            })
                                            .on_click(
                                                cx.listener(move |app, _, _, cx| {
                                                    app.set_track_ignored(id, false, cx)
                                                }),
                                            ),
                                        )
                                        .child(
                                            app.icon_button(
                                                ("delete-ignored", id as u64),
                                                "🗑",
                                                false,
                                            )
                                            .tooltip(|_, cx| {
                                                cx.new(|_| {
                                                    super::sidebar::SidebarTooltip(
                                                        tr("Удалить из хранилища…").into(),
                                                    )
                                                })
                                                .into()
                                            })
                                            .text_color(rgb(0xf29a9a))
                                            .on_click(
                                                cx.listener(move |app, _, window, cx| {
                                                    app.open_modal(
                                                        Modal::DeleteTrack(delete.clone()),
                                                        window,
                                                        cx,
                                                    );
                                                }),
                                            ),
                                        )
                                })
                                .collect()
                        }),
                    )
                    .h(px((count as f32 * 64.).min(440.))),
                );
        }
        section.into_any_element()
    }
}
