use super::components::SliderKind;
use super::*;

impl MusicApp {
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
            self.message = Some("Дождитесь завершения текущей операции".into());
            cx.notify();
            return;
        }
        let service = self.background_service.clone();
        self.run_job(
            "Готовим фон…",
            move || {
                let path = std::thread::Builder::new()
                    .name("shard-background-picker".into())
                    .spawn(|| {
                        rfd::FileDialog::new()
                            .set_title("Фон Shard")
                            .add_filter("Изображения", &["png", "jpg", "jpeg", "webp"])
                            .pick_file()
                    })?
                    .join()
                    .map_err(|_| anyhow::anyhow!("Не удалось открыть выбор изображения"))?;
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
            self.message = Some("Дождитесь завершения текущей операции".into());
            cx.notify();
            return;
        }
        let service = self.background_service.clone();
        self.run_job(
            "Применяем размытие…",
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
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(if scope==0 {"Фон приложения"}else{"Фон плейлиста"}))
            .child(div().text_sm().text_color(rgb(colors.muted)).child(if scope==0 {"PNG, JPEG или WebP, до 25 МБ. Фон используется везде, где не задан фон плейлиста."}else{"Собственный фон заменяет общий только в этом плейлисте."}))
            .child(self.button("choose-background","Выбрать изображение…",true)
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
                        .child("Размытие")
                        .child(format!("{blur} / 40")),
                )
                .child(self.slider(
                    "background-blur",
                    SliderKind::BackgroundBlur,
                    f32::from(blur) / 40.,
                    cx,
                ))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(colors.muted))
                        .child("Перетащите ползунок — размытие применится после отпускания."),
                )
                .child(
                    self.button(
                        "remove-background",
                        if scope == 0 {
                            "Убрать фон"
                        } else {
                            "Использовать общий фон"
                        },
                        false,
                    )
                    .on_click(cx.listener(move |app, _, _, cx| {
                        if app.busy.is_some() {
                            return;
                        }
                        let service = app.background_service.clone();
                        app.run_job(
                            "Убираем фон…",
                            move || service.remove(scope),
                            |app, (), _| app.blur_preview = None,
                            cx,
                        );
                    })),
                );
        } else {
            section = section.child(div().text_xs().text_color(rgb(colors.muted)).child(
                if scope == 0 {
                    "Сейчас используется фон выбранной темы."
                } else {
                    "Сейчас используется общий фон приложения."
                },
            ));
        }
        section.into_any_element()
    }
    pub(super) fn settings_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        div().id("settings-scroll").size_full().max_w(px(960.)).min_w_0().overflow_y_scroll().py_5()
            .flex().flex_col().gap_5()
            .child(div().flex_shrink_0().text_2xl().font_weight(FontWeight::BOLD).child("Настройки"))
            .children(self.notifications(cx))
            .children(self.busy.as_ref().map(|label|div().flex_shrink_0().text_sm().text_color(rgb(colors.accent)).child(label.clone())))
            .child(div().flex_shrink_0().p_4().rounded_xl().bg(self.surface(colors.panel)).border_1().border_color(rgb(colors.border))
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Тема приложения"))
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
            .child(div().flex_shrink_0().pb_4().text_xs().text_color(rgb(colors.muted)).child("Тема, фон и размытие сохраняются автоматически. Фон отдельного плейлиста меняется через «Оформление» в нём."))
            .into_any_element()
    }
}
