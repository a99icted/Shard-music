use super::components::SliderKind;
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsCategory {
    Customization,
    Performance,
    Audio,
    Storage,
    Ignoring,
    Language,
    Synchronization,
    ConnectedDevices,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CustomizationCategory {
    Themes,
    Backgrounds,
    Pets,
    Cursor,
}

impl MusicApp {
    fn audio_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let state = self.player.state();
        let mut choices = vec![(
            None,
            localized_format!("Системное по умолчанию", "System default"),
        )];
        choices.extend(
            state
                .output_devices
                .iter()
                .map(|name| (Some(name.clone()), name.clone())),
        );
        if let Some(name) = &self.preferences.audio_output
            && !state.output_devices.contains(name)
        {
            choices.push((
                Some(name.clone()),
                localized_format!("{name} (недоступно)", "{name} (unavailable)"),
            ));
        }
        let monitoring = self.preferences.audio_monitoring;
        let streaming = self.preferences.obs_stream;
        let mut view = div().flex_shrink_0().flex().flex_col().gap_3().p_4().rounded_xl()
            .bg(self.surface(colors.panel)).border_1().border_color(rgb(colors.border))
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD)
                .child(localized_format!("Аудиовыход и OBS", "Audio output and OBS")))
            .child(div().text_sm().text_color(rgb(colors.muted)).child(localized_format!(
                "Без устройств вывода музыка продолжает играть. При подключении устройства звук возвращается на текущей позиции.",
                "Music keeps playing without an output device. Reconnecting a device restores sound at the current position."
            )))
            .child(self.button("audio-monitoring", &localized_format!(
                "Звук на этом компьютере: {}", "Local playback: {}",
                if monitoring { tr("Включено") } else { tr("Выключено") }), monitoring)
                .on_click(cx.listener(|app, _, _, cx| {
                    app.preferences.audio_monitoring = !app.preferences.audio_monitoring;
                    app.player.set_monitoring(app.preferences.audio_monitoring);
                    app.save_preferences();
                    cx.notify();
                })))
            .children(state.active_output.as_ref().map(|name| div().text_sm()
                .text_color(rgb(colors.muted)).child(localized_format!("Сейчас используется: {name}", "Currently using: {name}"))))
            .children(state.device_less.then(|| div().text_sm().text_color(rgb(colors.muted))
                .child(localized_format!("Воспроизведение без локального аудиовыхода", "Playing without a local audio output"))))
            .children(choices.into_iter().enumerate().map(|(index, (device, label))| {
                self.button(("audio-output", index as u64), &label, monitoring && self.preferences.audio_output == device)
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.preferences.audio_output = device.clone();
                        app.player.set_output_device(device.clone());
                        if !app.preferences.audio_monitoring {
                            app.preferences.audio_monitoring = true;
                            app.player.set_monitoring(true);
                        }
                        app.save_preferences();
                        cx.notify();
                    }))
            }))
            .child(self.button("refresh-audio-outputs", &localized_format!("Обновить список", "Refresh devices"), false)
                .on_click(cx.listener(|app, _, _, _| app.player.refresh_output_devices())))
            .child(div().mt_3().font_weight(FontWeight::SEMIBOLD)
                .child(localized_format!("Захват в OBS", "OBS capture")))
            .child(div().text_sm().text_color(rgb(colors.muted)).child(localized_format!(
                "Захват аудио приложения: выберите Shard в OBS. Для этого нужен активный выход Windows — колонки, HDMI или виртуальный кабель. Уже установленный виртуальный выход можно выбрать в списке выше.",
                "Application audio capture: select Shard in OBS. This requires an active Windows output: speakers, HDMI or a virtual cable. An installed virtual output can be selected above."
            )))
            .child(self.button("obs-stream", &localized_format!(
                "Аудиопоток для OBS: {}", "Audio stream for OBS: {}",
                if streaming { tr("Включено") } else { tr("Выключено") }), streaming)
                .on_click(cx.listener(|app, _, _, cx| {
                    app.preferences.obs_stream = !app.preferences.obs_stream;
                    app.player.set_obs_stream(app.preferences.obs_stream);
                    app.save_preferences();
                    cx.notify();
                })))
            .child(div().text_sm().text_color(rgb(colors.muted)).child(localized_format!(
                "Аудиопоток работает даже без устройств вывода. В OBS добавьте «Источник медиа», отключите «Локальный файл» и вставьте адрес ниже. Формат входа: wav. Для стабильного подключения отключите «Закрывать файл при неактивности». Громкость Shard влияет и на поток; чтобы слушали только зрители, выключите «Звук на этом компьютере».",
                "The audio stream works without output devices. In OBS add a Media Source, uncheck Local File and paste the address below. Input format: wav. Uncheck Close file when inactive to keep the connection open. Shard volume also affects the stream; turn off Local playback if only viewers should hear it."
            )));
        if let Some(url) = state.obs_url {
            let copy = url.clone();
            view = view.child(div().text_sm().child(url)).child(
                self.button(
                    "copy-obs-url",
                    &localized_format!("Копировать адрес OBS", "Copy OBS address"),
                    false,
                )
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                })),
            );
        } else if streaming {
            view = view.child(div().text_sm().text_color(rgb(colors.muted)).child(localized_format!(
                "Запуск потока… Если адрес не появился, проверьте сообщение об ошибке и попробуйте выключить и включить поток.",
                "Starting stream… If no address appears, check the error message and toggle the stream off and on."
            )));
        }
        view.child(div().text_sm().text_color(rgb(colors.muted)).child(localized_format!(
            "Используйте один способ захвата Shard в каждой сцене OBS, чтобы звук не дублировался. Поток доступен только на этом компьютере.",
            "Use one Shard capture method per OBS scene to avoid duplicate audio. The stream is accessible only on this computer."
        ))).into_any_element()
    }

    pub(super) fn header_color_controls(&self, id: i64, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let playlist = self.playlists.iter().find(|p| p.id == id);
        let custom = playlist.and_then(|p| p.header_color);
        let inherited = playlist.and_then(|p| p.cover_color).unwrap_or(colors.hero);
        let active = custom.is_some();
        let mut controls = div().flex_shrink_0().flex().flex_col().gap_3().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(tr("Цвет шапки")),
                )
                .child(
                    div()
                        .id("header-color-mode")
                        .flex()
                        .items_center()
                        .gap_3()
                        .cursor_pointer()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(colors.muted))
                                .child(if active {
                                    tr("Свой цвет")
                                } else {
                                    tr("Цвет обложки")
                                }),
                        )
                        .child(
                            div()
                                .relative()
                                .w(px(44.))
                                .h(px(26.))
                                .rounded_full()
                                .bg(rgb(if active {
                                    colors.accent
                                } else {
                                    colors.button_hover
                                }))
                                .child(
                                    div()
                                        .absolute()
                                        .top(px(3.))
                                        .left(px(if active { 21. } else { 3. }))
                                        .size(px(20.))
                                        .rounded_full()
                                        .bg(rgb(if active { colors.bg } else { colors.text })),
                                ),
                        )
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.set_playlist_color(
                                id,
                                if active { None } else { Some(inherited) },
                                cx,
                            )
                        })),
                ),
        );
        if active {
            let color = self.header_color_state(id);
            controls = controls
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .size_8()
                                .flex_shrink_0()
                                .rounded_lg()
                                .bg(rgb(Self::header_color_value(color))),
                        )
                        .child(div().flex_1().min_w_0().child(self.input.clone())),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .w(px(92.))
                                .text_xs()
                                .text_color(rgb(colors.muted))
                                .child(tr("Оттенок")),
                        )
                        .child(self.slider("header-hue", SliderKind::HeaderHue, color.h, cx)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .w(px(92.))
                                .text_xs()
                                .text_color(rgb(colors.muted))
                                .child(tr("Насыщенность")),
                        )
                        .child(self.slider(
                            "header-saturation",
                            SliderKind::HeaderSaturation,
                            color.s,
                            cx,
                        )),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .w(px(92.))
                                .text_xs()
                                .text_color(rgb(colors.muted))
                                .child(tr("Яркость")),
                        )
                        .child(self.slider(
                            "header-lightness",
                            SliderKind::HeaderLightness,
                            color.l,
                            cx,
                        )),
                )
                .child(
                    self.button("apply-header-color", tr("Применить свой цвет"), false)
                        .on_click(
                            cx.listener(|app, _, window, cx| app.submit(&Submit, window, cx)),
                        ),
                );
        }
        controls.into_any_element()
    }

    pub(super) fn background_scope(&self) -> i64 {
        if let Some(Modal::PlaylistAppearance(id)) = self.modal {
            id
        } else {
            0
        }
    }
    pub(super) fn active_background(&self) -> Option<&Background> {
        let scope = if self.home_open || self.settings_open {
            0
        } else {
            self.selected_playlist.unwrap_or(0)
        };
        self.backgrounds
            .iter()
            .find(|b| b.scope == scope)
            .or_else(|| self.backgrounds.iter().find(|b| b.scope == 0))
    }
    pub(super) fn surface(&self, color: u32) -> Hsla {
        if self.active_background().is_some() {
            rgba((color << 8) | 0xd9).into()
        } else {
            rgb(color).into()
        }
    }
    pub(super) fn choose_background(&mut self, scope: i64, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            self.message = Some(tr("Дождитесь завершения текущей операции").into());
            cx.notify();
            return;
        }
        let service = self.background_service.clone();
        let video = self.background_animation.suspend(cx);
        self.run_job(
            tr("Готовим фон…"),
            move || {
                if let Some(video) = video {
                    video.stop();
                }
                let path = std::thread::Builder::new()
                    .name("shard-background-picker".into())
                    .spawn(|| {
                        rfd::FileDialog::new()
                            .set_title(tr("Фон Shard"))
                            .add_filter(
                                &localized_format!(
                                    "Фон: изображение, GIF или видео",
                                    "Background: image, GIF or video"
                                ),
                                &[
                                    "png", "jpg", "jpeg", "webp", "gif", "mp4", "webm", "mkv",
                                    "mov", "avi", "m4v",
                                ],
                            )
                            .pick_file()
                    })?
                    .join()
                    .map_err(|_| anyhow::anyhow!(tr("Не удалось открыть выбор изображения")))?;
                if let Some(path) = path {
                    service.set_media(scope, &path)?;
                }
                Ok(())
            },
            |_, (), _| {},
            cx,
        );
    }
    pub(super) fn apply_background_blur(&mut self, scope: i64, blur: u8, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            self.blur_preview = None;
            self.message = Some(tr("Дождитесь завершения текущей операции").into());
            cx.notify();
            return;
        }
        let service = self.background_service.clone();
        let video = self.background_animation.suspend(cx);
        self.run_job(
            tr("Применяем размытие…"),
            move || {
                if let Some(video) = video {
                    video.stop();
                }
                service.set_blur(scope, blur)
            },
            |_, (), _| {},
            cx,
        );
        self.blur_preview = None;
    }
    pub(super) fn background_controls(&self, scope: i64, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let background = self.backgrounds.iter().find(|b| b.scope == scope);
        let mut section=div().flex_shrink_0().flex().flex_col().gap_3().p_4().rounded_xl()
            .bg(self.surface(colors.panel)).border_1().border_color(rgb(colors.border))
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(if scope==0 {tr("Фон приложения")}else{tr("Фон плейлиста")}))
            .child(div().text_sm().text_color(rgb(colors.muted)).child(localized_format!(
                "PNG, JPEG, WebP — до 25 МБ; GIF и видео — до 250 МБ. Анимация зациклена и воспроизводится без звука.",
                "PNG, JPEG, WebP up to 25 MB; GIF and video up to 250 MB. Animation loops silently.")))
            .child(self.button("choose-background", &localized_format!("Выбрать изображение, GIF или видео…", "Choose image, GIF or video…"),true)
                .on_click(cx.listener(move |app,_,_,cx|app.choose_background(scope,cx))));
        if let Some(background) = background {
            let blur = self.blur_preview.unwrap_or(background.blur);
            section = section
                .child(
                    div()
                        .h(px(120.))
                        .flex_shrink_0()
                        .rounded_lg()
                        .overflow_hidden()
                        .child(
                            img(self.background_image(background))
                                .image_cache(&self.images)
                                .size_full()
                                .object_fit(ObjectFit::Cover),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .text_sm()
                        .child(tr("Размытие"))
                        .child(format!("{blur} / 40")),
                )
                .child(self.slider(
                    "background-blur",
                    SliderKind::BackgroundBlur,
                    f32::from(blur) / 40.,
                    cx,
                ))
                .child(div().text_xs().text_color(rgb(colors.muted)).child(tr(
                    "Перетащите ползунок — размытие применится после отпускания.",
                )))
                .child(
                    self.button(
                        "remove-background",
                        if scope == 0 {
                            tr("Убрать фон")
                        } else {
                            tr("Использовать общий фон")
                        },
                        false,
                    )
                    .on_click(cx.listener(move |app, _, _, cx| {
                        if app.busy.is_some() {
                            return;
                        }
                        let service = app.background_service.clone();
                        let video = app.background_animation.suspend(cx);
                        app.run_job(
                            tr("Убираем фон…"),
                            move || {
                                if let Some(video) = video {
                                    video.stop();
                                }
                                service.remove(scope)
                            },
                            |app, (), _| app.blur_preview = None,
                            cx,
                        );
                    })),
                );
        } else {
            section = section.child(div().text_xs().text_color(rgb(colors.muted)).child(
                if scope == 0 {
                    tr("Сейчас используется фон выбранной темы.")
                } else {
                    tr("Сейчас используется общий фон приложения.")
                },
            ));
        }
        section.into_any_element()
    }
    fn storage_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        div().flex().flex_col().gap_5()
            .child(
                div()
                    .flex_shrink_0()
                    .p_4()
                    .rounded_xl()
                    .bg(self.surface(colors.panel))
                    .border_1()
                    .border_color(rgb(colors.border))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(localized_format!("Хранилище музыки", "Music storage")),
                    )
                    .child(
                        div()
                            .mt_2()
                            .text_xs()
                            .text_color(rgb(colors.muted))
                            .child(self.data_directory.to_string_lossy().into_owned()),
                    )
                    .child(
                        self.button("change-library-folder", tr("Изменить хранилище"), false)
                            .mt_3()
                            .on_click(cx.listener(|app, _, _, cx| app.choose_storage(cx))),
                    )
                    .child(
                        self.button("open-library-folder", tr("Открыть папку хранилища"), false)
                            .mt_3()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(svg().path("icons/folder.svg").size_4())
                            .on_click(cx.listener(|app, _, _, cx| {
                                #[cfg(windows)]
                                let result = std::process::Command::new("explorer.exe")
                                    .arg(&app.data_directory)
                                    .spawn();
                                #[cfg(not(windows))]
                                let result = std::process::Command::new("xdg-open")
                                    .arg(&app.data_directory)
                                    .spawn();
                                if let Err(error) = result {
                                    app.error = Some(format!("{error:#}"));
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(
                self.button(
                    "refresh-artwork",
                    if self.artwork_cancel.is_some() {
                        tr("Обновляем обложки…")
                    } else {
                        tr("Подтянуть недостающие обложки")
                    },
                    false,
                )
                .on_click(cx.listener(|app, _, _, cx| {
                    if app.artwork_cancel.is_some() || app.storage_clearing || app.storage_moving {
                        return;
                    }
                    let token = Arc::new(AtomicBool::new(false));
                    app.artwork_cancel = Some(token.clone());
                    let service = app.tracks_service.clone();
                    let task = cx
                        .background_executor()
                        .spawn(async move { service.refresh_artwork(&token) });
                    cx.spawn(async move |this, cx| {
                        let result = task.await;
                        let _ = this.update(cx, |app, cx| {
                            app.artwork_cancel = None;
                            match result {
                                Ok(count) => {
                                    app.message = Some(localized_format!(
                                        "Обновлено обложек треков: {count}",
                                        "Track covers updated: {count}"
                                    ))
                                }
                                Err(error) => app.error = Some(format!("{error:#}")),
                            }
                            app.refresh(cx);
                        });
                    })
                    .detach();
                    cx.notify();
                })),
            )
            .child(div().p_4().rounded_xl().border_1().border_color(rgba(0xe8787855))
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(localized_format!("Очистка музыки", "Clear music")))
                .child(div().mt_2().text_sm().text_color(rgb(colors.muted)).child(localized_format!("Удалить все треки и их обложки из этого хранилища. Плейлисты останутся пустыми; оформление, настройки и статистика сохранятся.", "Delete all tracks and their covers from this storage. Playlists become empty; appearance, settings and listening totals are kept.")))
                .child(self.button("clear-storage", &localized_format!("Очистить хранилище…", "Clear storage…"), false).mt_3().text_color(rgb(0xf29a9a))
                    .on_click(cx.listener(|app,_,window,cx|app.open_modal(Modal::ClearStorage,window,cx)))))
            .into_any_element()
    }

    pub(super) fn settings_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let mut view = div()
            .id("settings-scroll")
            .size_full()
            .max_w(px(960.))
            .min_w_0()
            .overflow_y_scroll()
            .py_5()
            .flex()
            .flex_col()
            .gap_5()
            .child(
                div()
                    .flex_shrink_0()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .child(tr("Настройки")),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .gap_3()
                    .flex_wrap()
                    .child(
                        self.button(
                            "settings-customization",
                            tr("Кастомизация"),
                            self.settings_category == SettingsCategory::Customization,
                        )
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.settings_category = SettingsCategory::Customization;
                            cx.notify();
                        })),
                    )
                    .children(
                        [(
                            SettingsCategory::Performance,
                            localized_format!(
                                "Окно и производительность",
                                "Window and performance"
                            ),
                        )]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (category, label))| {
                            self.button(
                                ("settings-extra", i as u64),
                                &label,
                                self.settings_category == category,
                            )
                            .on_click(cx.listener(
                                move |app, _, _, cx| {
                                    app.settings_category = category;
                                    cx.notify();
                                },
                            ))
                        }),
                    )
                    .child(
                        self.button(
                            "settings-audio",
                            &localized_format!("Звук", "Audio"),
                            self.settings_category == SettingsCategory::Audio,
                        )
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.settings_category = SettingsCategory::Audio;
                            app.player.refresh_output_devices();
                            cx.notify();
                        })),
                    )
                    .child(
                        self.button(
                            "settings-storage",
                            &localized_format!("Хранилище", "Storage"),
                            self.settings_category == SettingsCategory::Storage,
                        )
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.settings_category = SettingsCategory::Storage;
                            cx.notify();
                        })),
                    )
                    .child(
                        self.button(
                            "settings-language",
                            tr("Язык"),
                            self.settings_category == SettingsCategory::Language,
                        )
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.settings_category = SettingsCategory::Language;
                            cx.notify();
                        })),
                    )
                    .child(
                        self.button(
                            "settings-ignoring",
                            tr("Игнорирование"),
                            self.settings_category == SettingsCategory::Ignoring,
                        )
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.settings_category = SettingsCategory::Ignoring;
                            cx.notify();
                        })),
                    )
                    .child(
                        self.button(
                            "settings-sync",
                            tr("Синхронизация"),
                            self.settings_category == SettingsCategory::Synchronization,
                        )
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.settings_category = SettingsCategory::Synchronization;
                            cx.notify();
                        })),
                    )
                    .children(self.sync_service.is_some().then(|| {
                        self.button(
                            "settings-devices",
                            tr("Подключённые устройства"),
                            self.settings_category == SettingsCategory::ConnectedDevices,
                        )
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.settings_category = SettingsCategory::ConnectedDevices;
                            cx.notify();
                        }))
                    })),
            )
            .children(self.notifications(cx))
            .children(self.busy.as_ref().map(|label| {
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(rgb(colors.accent))
                    .child(label.clone())
            }));
        match self.settings_category {
            SettingsCategory::Performance => {
                view = view.child(self.performance_settings(cx));
            }
            SettingsCategory::Audio => {
                view = view.child(self.audio_settings(cx));
            }
            SettingsCategory::ConnectedDevices => {
                view = view.child(self.connected_devices_view(cx));
            }
            SettingsCategory::Ignoring => {
                view = view.child(self.ignored_settings(cx));
            }
            SettingsCategory::Storage => {
                view = view.child(self.storage_settings(cx));
            }
            SettingsCategory::Synchronization => {
                view = view.child(self.sync_settings(cx));
            }
            SettingsCategory::Language => {
                view = view.child(
                    div()
                        .flex_shrink_0()
                        .p_4()
                        .rounded_xl()
                        .bg(self.surface(colors.panel))
                        .border_1()
                        .border_color(rgb(colors.border))
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(tr("Язык приложения")),
                        )
                        .child(
                            div()
                                .mt_3()
                                .flex()
                                .gap_3()
                                .child(
                                    self.button(
                                        "language-russian",
                                        "Русский",
                                        self.preferences.language == shard::i18n::Language::Russian,
                                    )
                                    .on_click(cx.listener(
                                        |app, _, _, cx| {
                                            app.set_language(shard::i18n::Language::Russian, cx)
                                        },
                                    )),
                                )
                                .child(
                                    self.button(
                                        "language-english",
                                        "English",
                                        self.preferences.language == shard::i18n::Language::English,
                                    )
                                    .on_click(cx.listener(
                                        |app, _, _, cx| {
                                            app.set_language(shard::i18n::Language::English, cx)
                                        },
                                    )),
                                ),
                        ),
                );
            }
            SettingsCategory::Customization => {
                view = view.child(
                    div().flex().flex_shrink_0().gap_2().flex_wrap().children(
                        [
                            (
                                CustomizationCategory::Themes,
                                localized_format!("Темы", "Themes"),
                            ),
                            (
                                CustomizationCategory::Backgrounds,
                                localized_format!("Фоны", "Backgrounds"),
                            ),
                            (
                                CustomizationCategory::Pets,
                                localized_format!("Питомцы", "Pets"),
                            ),
                            (
                                CustomizationCategory::Cursor,
                                localized_format!("Курсор", "Cursor"),
                            ),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (category, label))| {
                            self.button(
                                ("customization-tab", i as u64),
                                &label,
                                self.customization_category == category,
                            )
                            .on_click(cx.listener(
                                move |app, _, _, cx| {
                                    app.customization_category = category;
                                    cx.notify();
                                },
                            ))
                        }),
                    ),
                );
                match self.customization_category {
                    CustomizationCategory::Backgrounds => {
                        view = view.child(self.background_controls(0, cx));
                    }
                    CustomizationCategory::Pets => {
                        view = view.child(self.pet_settings(cx));
                    }
                    CustomizationCategory::Cursor => {
                        view = view.child(self.cursor_settings(cx));
                    }
                    CustomizationCategory::Themes => {
                        view = view.child(div().flex_shrink_0().p_4().rounded_xl().bg(self.surface(colors.panel)).border_1().border_color(rgb(colors.border))
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(tr("Тема приложения")))
                    .child(div().mt_3().flex().flex_wrap().gap_3().children([Theme::Forest,Theme::Onyx,Theme::PickMe,Theme::Cosmic].into_iter().enumerate().map(|(index,theme)| {
                        let palette=theme.palette();
                        div().id(("settings-theme",index as u64)).relative().w(px(144.)).p_3().rounded_lg().cursor_pointer()
                            .bg(rgb(palette.panel)).border_2().border_color(rgb(if self.preferences.theme==theme {palette.accent}else{palette.border}))
                            .hover(move |s|s.border_color(rgb(palette.accent)))
                            .child(div().flex().gap_2().children([palette.bg,palette.button,palette.accent].into_iter().map(|color|div().size_5().rounded_full().bg(rgb(color)))))
                            .child(div().mt_3().text_color(rgb(palette.text)).child(theme.name()))
                            .child(self.button_feedback(8.))
                            .on_click(cx.listener(move |app,_,_,cx|app.set_theme(theme,cx)))
                    }))))

                    .child(div().flex_shrink_0().pb_4().text_xs().text_color(rgb(colors.muted)).child(tr("Настройки сохраняются автоматически. Фон плейлиста меняется кнопкой карандаша в его шапке.")));
                    }
                }
            }
        }
        view.into_any_element()
    }

    pub(super) fn header_color_state(&self, id: i64) -> Hsla {
        self.header_color_draft
            .filter(|(draft_id, _)| *draft_id == id)
            .map(|(_, color)| color)
            .unwrap_or_else(|| {
                rgb(self
                    .playlists
                    .iter()
                    .find(|p| p.id == id)
                    .and_then(|p| p.header_color.or(p.cover_color))
                    .unwrap_or(self.colors().hero))
                .into()
            })
    }
    pub(super) fn header_color_value(color: Hsla) -> u32 {
        let color = color.to_rgb();
        ((color.r * 255.).round() as u32) << 16
            | ((color.g * 255.).round() as u32) << 8
            | (color.b * 255.).round() as u32
    }
}

impl MusicApp {
    fn performance_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let active = self.preferences.refresh_inactive;
        div().flex_shrink_0().p_4().rounded_xl().bg(self.surface(colors.panel)).border_1().border_color(rgb(colors.border))
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(localized_format!("Обновление в неактивном окне", "Refresh while window is inactive")))
            .child(div().mt_3().text_sm().text_color(rgb(colors.muted)).child(localized_format!(
                "Обновляет время, питомца и видеофон, когда фокус находится в другом приложении. При выключении анимации приостанавливаются для экономии ресурсов. Музыка продолжает играть.",
                "Updates time, pets and video backgrounds while another app has focus. When disabled, animations pause to save resources. Music keeps playing.")))
            .child(self.button("refresh-inactive", &localized_format!("Обновление в неактиве: {}", "Refresh while inactive: {}", if active { tr("Включено") } else { tr("Выключено") }), active)
                .mt_3().on_click(cx.listener(|app, _, _, cx| { app.preferences.refresh_inactive = !app.preferences.refresh_inactive; app.save_preferences(); cx.notify(); })))
            .into_any_element()
    }
    fn pet_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        use shard::pets::ProgressPet;
        let colors = self.colors();
        let selected = self.preferences.progress_pet;
        let mut view = div().flex_shrink_0().p_4().rounded_xl().bg(self.surface(colors.panel)).border_1().border_color(rgb(colors.border)).flex().flex_col().gap_4()
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(localized_format!("Питомец на полосе воспроизведения", "Pet on the playback bar")))
            .child(div().text_sm().text_color(rgb(colors.muted)).child(localized_format!("Компактная анимация справа над прогрессом. Вариант анимации запоминается отдельно для каждого питомца.", "A compact animation at the right end of the progress bar. Each pet remembers its own animation variant.")))
            .child(div().flex().flex_wrap().gap_3().children(ProgressPet::ALL.into_iter().enumerate().map(|(i, pet)| {
                self.button(("pet-choice", i as u64), pet.label(), pet == selected)
                    .on_click(cx.listener(move |app, _, _, cx| { app.preferences.progress_pet = pet; app.save_preferences(); cx.notify(); }))
            })));
        if let Some(picker) = &self.pet_picker {
            view = view.child(picker.clone());
        }
        view.child(
            div()
                .text_sm()
                .text_color(rgb(colors.muted))
                .child(localized_format!(
                    "Полный экран трека: кнопка ⛶ в плеере или F11. Выход — Escape.",
                    "Fullscreen track: the ⛶ player button or F11. Press Escape to exit."
                )),
        )
        .into_any_element()
    }
    fn cursor_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        use shard::pets::CursorPet;
        let colors = self.colors();
        let selected = self.preferences.cursor_pet;
        div().flex_shrink_0().flex().flex_col().gap_4().p_4().rounded_xl()
            .bg(self.surface(colors.panel)).border_1().border_color(rgb(colors.border))
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD)
                .child(localized_format!("Курсор", "Cursor")))
            .child(div().text_sm().text_color(rgb(colors.muted)).child(localized_format!(
                "Котёнок, жабка или поросёнок будут догонять указатель внутри окна приложения. Когда мышь останавливается, питомец подходит ближе и замирает. Он не мешает нажимать кнопки и перематывать музыку.",
                "A kitten, frog or piglet follows the pointer inside the app window. When the pointer stops, your companion catches up and rests. Buttons and seeking work through the pet."
            )))
            .child(div().flex().flex_wrap().gap_3().children(CursorPet::ALL.into_iter().enumerate().map(|(i, pet)| {
                self.button(("cursor-pet-choice", i as u64), pet.label(), pet == selected)
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.preferences.cursor_pet = pet;
                        app.save_preferences();
                        cx.notify();
                    }))
            })))
            .child(div().text_sm().text_color(rgb(colors.muted)).child(localized_format!(
                "Выбор запоминается отдельно от питомца на полосе воспроизведения. В неактивном окне погоня приостанавливается.",
                "This choice is saved separately from the progress-bar pet. Chasing pauses while the window is inactive."
            )))
            .child(div().flex().items_center().justify_between()
                .child(localized_format!("Скорость питомца", "Companion speed"))
                .child(format!("{}%", self.preferences.cursor_pet_speed)))
            .child(self.slider("cursor-pet-speed", SliderKind::CursorPetSpeed,
                (f32::from(self.preferences.cursor_pet_speed) - 20.) / 130., cx))
            .child(div().text_sm().text_color(rgb(colors.muted)).child(localized_format!(
                "От 20% до 150%. По умолчанию — 50%. Скорость перемещения меняется плавно, частота кадров анимации остаётся прежней.",
                "20% to 150%. Default: 50%. Movement speed changes smoothly; the animation frame rate stays the same."
            )))
            .into_any_element()
    }
}
