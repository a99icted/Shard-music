use super::*;

impl MusicApp {
    pub(super) fn storage_is_busy(&self) -> bool {
        self.library_loading
            || self.thumbnail_cancel.is_some()
            || self.storage_clearing
            || self.storage_moving
            || self.busy.is_some()
            || self.active_download.is_some()
            || !self.pending_downloads.is_empty()
            || self.artwork_cancel.is_some()
            || !self.playlist_name_pending.is_empty()
            || !self.favorite_pending.is_empty()
            || self.sync_duplicate_requests.is_some()
            || self
                .sync_service
                .as_ref()
                .is_some_and(|s| s.is_transferring())
    }

    pub(super) fn clear_storage(&mut self, cx: &mut Context<Self>) {
        if self.storage_is_busy() {
            self.error = Some(localized_format!(
                "Дождитесь завершения операций с библиотекой и синхронизации.",
                "Wait for library operations and synchronization to finish."
            ));
            self.modal = None;
            cx.notify();
            return;
        }
        self.storage_clearing = true;
        self.library_generation += 1; // Discard any pre-clear library snapshot.
        self.modal = None;
        self.error = None;
        let sync = self.sync_service.take();
        let player = self.player.clone();
        let root = self.data_directory.clone();
        let task = cx.background_executor().spawn(async move {
            if let Some(sync) = sync {
                sync.stop();
            }
            player.set_library(vec![], false);
            player.stop();
            player.flush_and_wait();
            shard::storage::clear_music(&Database::open(&root)?)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                app.storage_clearing = false;
                app.image_cache_dirty = true;
                match result {
                    Ok(count) => {
                        app.message = Some(localized_format!(
                            "Хранилище очищено. Удалено треков: {count}",
                            "Storage cleared. Tracks deleted: {count}"
                        ))
                    }
                    Err(error) => app.error = Some(format!("{error:#}")),
                }
                app.refresh_statistics(cx);
                app.refresh(cx);
                if app.preferences.sync_enabled {
                    app.start_sync(cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn choose_storage(&mut self, cx: &mut Context<Self>) {
        if self.updates.downloading {
            self.error = Some(localized_format!(
                "Дождитесь завершения обновления приложения.",
                "Wait for the application update to finish."
            ));
            cx.notify();
            return;
        }
        if shard::app_identity::data_directory_override().is_some() {
            self.error = Some(if shard::app_identity::IS_TEST {
                localized_format!(
                    "Путь задан переменной окружения SHARD_TEST_DATA_DIR. Измените её, чтобы выбрать другое хранилище.",
                    "The path is set by SHARD_TEST_DATA_DIR. Change it to use another storage location."
                )
            } else {
                localized_format!(
                    "Путь задан переменной окружения SHARD_DATA_DIR / LOZEX_DATA_DIR. Измените её, чтобы выбрать другое хранилище.",
                    "The path is set by SHARD_DATA_DIR / LOZEX_DATA_DIR. Change the environment variable to use another storage location."
                )
            });
            cx.notify();
            return;
        }
        if self.storage_is_busy() {
            self.error = Some(localized_format!(
                "Дождитесь завершения загрузок, обновления обложек и синхронизации перед переносом.",
                "Wait for downloads, artwork updates and synchronization before moving storage."
            ));
            cx.notify();
            return;
        }
        let task = cx.background_executor().spawn(async {
            std::thread::Builder::new()
                .name("shard-storage-picker".into())
                .spawn(|| {
                    rfd::FileDialog::new()
                        .set_title(tr("Выберите папку для хранилища SHARD"))
                        .pick_folder()
                })?
                .join()
                .map_err(|_| anyhow::anyhow!("Storage picker failed"))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(Some(parent)) => {
                        app.modal = Some(Modal::MoveStorage(
                            parent.join(shard::app_identity::STORAGE_NAME),
                        ))
                    }
                    Ok(None) => {}
                    Err(error) => app.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn move_storage(&mut self, destination: PathBuf, cx: &mut Context<Self>) {
        if self.storage_is_busy() {
            self.error = Some(localized_format!(
                "Сначала завершите текущие операции с библиотекой.",
                "Finish current library operations first."
            ));
            cx.notify();
            return;
        }
        self.storage_moving = true;
        self.storage_ready = false;
        self.error = None;
        self.modal = None;
        let source = self.data_directory.clone();
        let preferences = self.preferences.clone();
        let writer = self.preferences_writer.clone();
        let player = self.player.clone();
        let sync = self.sync_service.take();
        let video = self.background_animation.suspend(cx);
        let task = cx.background_executor().spawn(async move {
            if let Some(video) = video {
                video.stop();
            }
            if let Some(sync) = sync {
                sync.stop();
            }
            player.stop();
            player.flush_and_wait();
            writer.flush_and_wait();
            shard::storage::relocate_library(
                &source,
                &destination,
                &shard::storage::config_directory()?,
                &preferences,
            )
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(_) => {
                        app.storage_ready = true;
                        app.restart_after_move(cx);
                    }
                    Err(error) => {
                        app.storage_moving = false;
                        app.error = Some(format!("{error:#}"));
                        if app.preferences.sync_enabled {
                            app.start_sync(cx);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn restart_after_move(&mut self, cx: &mut Context<Self>) {
        let result = std::env::current_exe().and_then(|exe| {
            std::process::Command::new(exe)
                .arg("--restart-after")
                .arg(std::process::id().to_string())
                .spawn()
        });
        match result {
            Ok(_) => cx.quit(),
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                cx.notify();
            }
        }
    }

    pub(super) fn moving_storage_view(&self, cx: &mut Context<Self>) -> Div {
        let colors = self.colors();
        div().size_full().bg(rgb(colors.bg)).text_color(rgb(colors.text)).font_family("Segoe UI")
            .flex().flex_col().items_center().justify_center().gap_4().p_6()
            .child(self.brand_mark(64.))
            .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(if self.storage_ready{
                localized_format!("Хранилище перенесено", "Storage moved")
            }else{localized_format!("Переносим вашу музыку…", "Moving your music…")}))
            .children((!self.storage_ready).then(||div().w(px(180.)).h(px(3.)).rounded_full().bg(rgb(colors.accent))
                .with_animation("storage-progress",Animation::new(Duration::from_millis(1200)).repeat().with_easing(bounce(ease_in_out)),|bar,t|bar.opacity(0.25+0.75*t))))
            .child(div().text_sm().text_color(rgb(colors.muted)).child(localized_format!("После переноса SHARD откроется заново. Исходная папка останется резервной копией.", "SHARD will reopen after the move. The original folder stays as a backup.")))
            .children(self.error.as_ref().map(|error|div().text_sm().child(error.clone())))
            .children(self.storage_ready.then(||self.button("restart-moved-library",tr("Перезапустить SHARD"),true)
                .on_click(cx.listener(|app,_,_,cx|app.restart_after_move(cx)))))
    }
}
