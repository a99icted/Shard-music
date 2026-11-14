use super::components::SliderKind;
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsCategory {
    Customization,
    Language,
}

impl MusicApp {
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
        self.run_job(
            tr("Готовим фон…"),
            move || {
                let path = std::thread::Builder::new()
                    .name("shard-background-picker".into())
                    .spawn(|| {
                        rfd::FileDialog::new()
                            .set_title(tr("Фон Shard"))
                            .add_filter(tr("Изображения"), &["png", "jpg", "jpeg", "webp"])
                            .pick_file()
                    })?
                    .join()
                    .map_err(|_| anyhow::anyhow!(tr("Не удалось открыть выбор изображения")))?;
                if let Some(path) = path {
                    service.set_image(scope, &path)?;
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
        self.run_job(
            tr("Применяем размытие…"),
            move || service.set_blur(scope, blur),
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
            .child(div().text_sm().text_color(rgb(colors.muted)).child(if scope==0 {tr("PNG, JPEG или WebP, до 25 МБ. Фон используется везде, где не задан фон плейлиста.")}else{tr("Собственный фон заменяет общий только в этом плейлисте.")}))
            .child(self.button("choose-background",tr("Выбрать изображение…"),true)
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
                            img(self.data_directory.join(&background.display_path))
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
                        app.run_job(
                            tr("Убираем фон…"),
                            move || service.remove(scope),
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
                    ),
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
                view = view.child(div().flex_shrink_0().p_4().rounded_xl().bg(self.surface(colors.panel)).border_1().border_color(rgb(colors.border))
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(tr("Тема приложения")))
                    .child(div().mt_3().flex().flex_wrap().gap_3().children([Theme::Forest,Theme::Onyx,Theme::PickMe,Theme::Cosmic].into_iter().enumerate().map(|(index,theme)| {
                        let palette=theme.palette();
                        div().id(("settings-theme",index as u64)).w(px(144.)).p_3().rounded_lg().cursor_pointer()
                            .bg(rgb(palette.panel)).border_2().border_color(rgb(if self.preferences.theme==theme {palette.accent}else{palette.border}))
                            .hover(move |s|s.border_color(rgb(palette.accent)))
                            .child(div().flex().gap_2().children([palette.bg,palette.button,palette.accent].into_iter().map(|color|div().size_5().rounded_full().bg(rgb(color)))))
                            .child(div().mt_3().text_color(rgb(palette.text)).child(theme.name()))
                            .on_click(cx.listener(move |app,_,_,cx|app.set_theme(theme,cx)))
                    }))))
                    .child(self.background_controls(0,cx))
                    .child(div().flex_shrink_0().pb_4().text_xs().text_color(rgb(colors.muted)).child(tr("Настройки сохраняются автоматически. Фон плейлиста меняется кнопкой карандаша в его шапке.")));
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
