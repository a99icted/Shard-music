use super::*;
use shard::updates::{AvailableUpdate, DownloadProgress, DownloadedUpdate};

pub(super) struct UpdateUi {
    pub(super) available: Option<AvailableUpdate>,
    pub(super) checking: bool,
    pub(super) downloading: bool,
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) automatic_checks: Option<Task<()>>,
    checked: bool,
    error: Option<String>,
    progress: Option<DownloadProgress>,
    progress_receiver: Option<mpsc::Receiver<DownloadProgress>>,
    ready: Option<DownloadedUpdate>,
}

impl Default for UpdateUi {
    fn default() -> Self {
        Self {
            available: None,
            checking: false,
            downloading: false,
            cancel: Arc::new(AtomicBool::new(false)),
            automatic_checks: None,
            checked: false,
            error: None,
            progress: None,
            progress_receiver: None,
            ready: None,
        }
    }
}

impl MusicApp {
    pub(super) fn schedule_update_checks(&mut self, cx: &mut Context<Self>) {
        if !cfg!(windows) || shard::app_identity::IS_TEST {
            return;
        }
        self.updates.automatic_checks = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            loop {
                if this
                    .update(cx, |app, cx| {
                        if app.preferences.update_notifications {
                            app.check_updates(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_secs(6 * 60 * 60))
                    .await;
            }
        }));
    }

    pub(super) fn check_updates(&mut self, cx: &mut Context<Self>) {
        if self.updates.checking || self.updates.downloading {
            return;
        }
        self.updates.checking = true;
        self.updates.error = None;
        let task = cx
            .background_executor()
            .spawn(async { shard::updates::check(env!("CARGO_PKG_VERSION")) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                app.updates.checking = false;
                match result {
                    Ok(update) => {
                        app.updates.checked = true;
                        if app.updates.ready.as_ref().is_some_and(|ready| {
                            update
                                .as_ref()
                                .is_none_or(|next| next.version != ready.version)
                        }) {
                            app.updates.ready = None;
                        }
                        app.updates.available = update;
                    }
                    Err(error) => {
                        app.updates.error = Some(localized_format!(
                            "Не удалось проверить обновления. Попробуйте позже. {error:#}",
                            "Could not check for updates. Try again later. {error:#}"
                        ))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn suppress_update_notifications(&mut self, cx: &mut Context<Self>) {
        self.preferences.update_notifications = false;
        self.save_preferences();
        cx.notify();
    }

    pub(super) fn poll_update_progress(&mut self) -> bool {
        let mut changed = false;
        if let Some(receiver) = &self.updates.progress_receiver {
            while let Ok(progress) = receiver.try_recv() {
                self.updates.progress = Some(progress);
                changed = true;
            }
        }
        changed
    }

    pub(super) fn download_application_update(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        if self.updates.downloading {
            return;
        }
        if self.updates.ready.is_some() {
            self.install_application_update(cx);
            return;
        }
        let Some(update) = self.updates.available.clone() else {
            return;
        };
        if shard::updates::installation_directory().is_err() {
            self.updates.error = Some(localized_format!(
                "Автоматическая установка доступна в обычной установленной версии Shard. Откройте страницу релиза и установите её вручную.",
                "Automatic installation is available in the installed stable edition of Shard. Open the release page to install it manually."
            ));
            self.settings_open = true;
            self.settings_category = settings::SettingsCategory::Updates;
            cx.notify();
            return;
        }
        self.updates.downloading = true;
        self.updates.error = None;
        self.updates.progress = Some(DownloadProgress {
            received: 0,
            total: update.size,
        });
        self.updates.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.updates.cancel.clone();
        let (sender, receiver) = mpsc::channel();
        self.updates.progress_receiver = Some(receiver);
        let task = cx.background_executor().spawn(async move {
            shard::updates::download(&update, &cancel, |progress| {
                let _ = sender.send(progress);
            })
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                app.updates.downloading = false;
                app.updates.progress_receiver = None;
                match result {
                    Ok(installer) if !app.updates.cancel.load(Ordering::Relaxed) => {
                        app.updates.ready = Some(installer);
                        app.install_application_update(cx);
                    }
                    Ok(_) => app.updates.progress = None,
                    Err(error) => {
                        if !app.updates.cancel.load(Ordering::Relaxed) {
                            app.updates.error = Some(localized_format!(
                                "Не удалось скачать обновление: {error:#}",
                                "Could not download the update: {error:#}"
                            ));
                        }
                        app.updates.progress = None;
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn install_application_update(&mut self, cx: &mut Context<Self>) {
        if self.storage_is_busy() {
            self.updates.error = Some(localized_format!(
                "Обновление скачано. Завершите операции с библиотекой и синхронизацию, затем нажмите «Установить обновление».",
                "The update is downloaded. Finish library operations and synchronization, then click Install update."
            ));
            self.settings_open = true;
            self.settings_category = settings::SettingsCategory::Updates;
            cx.notify();
            return;
        }
        let directory = match shard::updates::installation_directory() {
            Ok(directory) => directory,
            Err(error) => {
                self.updates.error = Some(format!("{error:#}"));
                cx.notify();
                return;
            }
        };
        let Some(installer) = self.updates.ready.take() else {
            return;
        };
        match installer.launch(&directory) {
            Ok(()) => cx.quit(),
            Err(error) => {
                self.updates.error = Some(localized_format!(
                    "Не удалось запустить установку обновления: {error:#}",
                    "Could not start the update installer: {error:#}"
                ));
                cx.notify();
            }
        }
    }

    fn update_progress_label(&self) -> String {
        let progress = self.updates.progress.unwrap_or(DownloadProgress {
            received: 0,
            total: 0,
        });
        let percent = if progress.total == 0 {
            0
        } else {
            progress.received * 100 / progress.total
        };
        localized_format!(
            "Скачивание обновления: {percent}%",
            "Downloading update: {percent}%"
        )
    }

    pub(super) fn update_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.preferences.update_notifications {
            return None;
        }
        let update = self.updates.available.as_ref()?;
        let colors = self.colors();
        let mut card = div()
            .id("update-notice")
            .flex_shrink_0()
            .w_full()
            .p_3()
            .rounded_lg()
            .bg(rgb(colors.active))
            .border_1()
            .border_color(rgb(colors.accent))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(localized_format!(
                        "Доступна версия {}",
                        "Version {} available",
                        update.version
                    )),
            );
        if self.updates.downloading {
            card = card.child(div().text_xs().child(self.update_progress_label()));
        } else {
            card = card.child(
                self.button(
                    "notice-update",
                    &localized_format!("Обновить", "Update"),
                    true,
                )
                .text_xs()
                .on_click(cx.listener(|app, _, window, cx| {
                    app.open_modal(Modal::ConfirmUpdate, window, cx)
                })),
            );
        }
        card = card.child(
            self.button(
                "hide-update-notice",
                &localized_format!("Больше не показывать", "Don't show again"),
                false,
            )
            .text_xs()
            .on_click(cx.listener(|app, _, _, cx| app.suppress_update_notifications(cx))),
        );
        Some(card.into_any_element())
    }

    pub(super) fn floating_update_notice(
        &self,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let compact = viewport.width < px(1100.) || viewport.height < px(700.);
        if (!self.preferences.sidebar_collapsed && !compact) || self.now_playing_open {
            return None;
        }
        self.update_notice(cx).map(|notice| {
            div()
                .absolute()
                .left(px(self.sidebar_width(compact) + 8.))
                .bottom(px(112.))
                .w(px(244.))
                .occlude()
                .shadow_lg()
                .child(notice)
                .into_any_element()
        })
    }

    pub(super) fn update_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let mut card = div().flex_shrink_0().p_4().rounded_xl().bg(self.surface(colors.panel))
            .border_1().border_color(rgb(colors.border)).flex().flex_col().gap_3()
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(localized_format!("Обновление", "Updates")))
            .child(div().text_sm().text_color(rgb(colors.muted)).child(localized_format!(
                "Установлена версия {}", "Installed version {}", shard::app_identity::VERSION)))
            .child(self.button("toggle-update-notifications", &localized_format!(
                "Оповещения об обновлениях: {}", "Update notifications: {}",
                if self.preferences.update_notifications { tr("Включено") } else { tr("Выключено") }
            ), self.preferences.update_notifications).on_click(cx.listener(|app, _, _, cx| {
                app.preferences.update_notifications = !app.preferences.update_notifications;
                app.save_preferences();
                if app.preferences.update_notifications { app.check_updates(cx); }
                cx.notify();
            })))
            .child(div().text_sm().text_color(rgb(colors.muted)).child(localized_format!(
                "Обновления проверяются при запуске и каждые 6 часов. Проверить вручную можно и при выключенных оповещениях.",
                "Updates are checked at startup and every 6 hours. Manual checks remain available when notifications are off."
            )))
            .child(self.button("check-updates", &if self.updates.checking {
                localized_format!("Проверяем обновления…", "Checking for updates…")
            } else { localized_format!("Проверить обновление", "Check for updates") }, false)
                .on_click(cx.listener(|app, _, _, cx| app.check_updates(cx))));
        if let Some(update) = &self.updates.available {
            card = card.child(div().text_sm().child(localized_format!(
                "Доступна версия {} · {:.1} МБ",
                "Version {} available · {:.1} MB",
                update.version,
                update.size as f64 / 1048576.0
            )));
            if self.updates.downloading {
                card = card
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(colors.accent))
                            .child(self.update_progress_label()),
                    )
                    .child(
                        self.button("cancel-update-download", tr("Отмена"), false)
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.updates.cancel.store(true, Ordering::Relaxed);
                                cx.notify();
                            })),
                    );
            } else {
                let label = if self.updates.ready.is_some() {
                    localized_format!("Установить обновление", "Install update")
                } else {
                    localized_format!("Скачать обновление", "Download update")
                };
                card = card.child(self.button("download-update", &label, true).on_click(
                    cx.listener(|app, _, window, cx| {
                        app.open_modal(Modal::ConfirmUpdate, window, cx)
                    }),
                ));
            }
            let url = update.release_url.clone();
            card = card.child(
                self.button(
                    "update-release-page",
                    &localized_format!("Что нового", "What's new"),
                    false,
                )
                .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url))),
            );
        } else if self.updates.checked && !self.updates.checking && self.updates.error.is_none() {
            card = card.child(div().text_sm().child(localized_format!(
                "Установлена последняя версия",
                "You have the latest version"
            )));
        }
        if let Some(error) = &self.updates.error {
            card = card.child(
                div()
                    .text_sm()
                    .text_color(rgb(0xf29a9a))
                    .child(error.clone()),
            );
        }
        card.into_any_element()
    }
}
