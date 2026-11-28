use super::*;

pub(super) struct DownloadRequest {
    pub url: String,
    pub collection: Option<shard::youtube::search::CollectionDownload>,
}
impl From<String> for DownloadRequest {
    fn from(url: String) -> Self {
        Self {
            url,
            collection: None,
        }
    }
}

pub(super) struct DownloadResult {
    pub url: String,
    pub message: String,
    pub details: Option<String>,
}

impl MusicApp {
    pub(super) fn start_next_download(&mut self, cx: &mut Context<Self>) {
        if self.active_download.is_some() {
            return;
        }
        let Some(request) = self.pending_downloads.pop_front() else {
            return;
        };
        self.active_download_collection = request.collection.clone();
        let options = shard::services::DownloadOptions {
            warn_duplicates: self.preferences.download_warn_duplicates,
            ignore_duplicates: self.preferences.download_ignore_duplicates,
        };
        self.launch_download(
            request.url,
            move |service, url, cancel, sender, requests| {
                let progress = |progress| {
                    let _ = sender.send(progress);
                };
                let decide = |duplicates: &[shard::sync::DuplicateTrack]| {
                    let (response, receiver) = mpsc::channel();
                    requests
                        .send(duplicates::DuplicateRequest {
                            tracks: duplicates.to_vec(),
                            response,
                            download: true,
                        })
                        .map_err(|_| anyhow::anyhow!(tr("Загрузка остановлена")))?;
                    loop {
                        anyhow::ensure!(
                            !cancel.load(Ordering::Relaxed),
                            tr("Загрузка остановлена")
                        );
                        match receiver.recv_timeout(Duration::from_millis(200)) {
                            Ok(Some(decisions)) => return Ok(decisions),
                            Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                                cancel.store(true, Ordering::Relaxed);
                                anyhow::bail!(tr("Загрузка остановлена"));
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                        }
                    }
                };
                if let Some(collection) = request.collection {
                    service.download_search_collection_with_options(
                        &collection,
                        &cancel,
                        progress,
                        options,
                        decide,
                    )
                } else {
                    service
                        .download_collection_with_options(&url, &cancel, progress, options, decide)
                }
            },
            cx,
        );
    }

    pub(super) fn launch_download(
        &mut self,
        url: String,
        job: impl FnOnce(
            TrackService,
            String,
            Arc<AtomicBool>,
            mpsc::Sender<YoutubeProgress>,
            mpsc::Sender<duplicates::DuplicateRequest>,
        ) -> Result<shard::youtube::YoutubeDownloadReport>
        + Send
        + 'static,
        cx: &mut Context<Self>,
    ) {
        self.active_download = Some(url.clone());
        self.job_progress = Some(YoutubeProgress {
            completed: 0,
            total: 0,
            current_title: None,
        });
        let service = self.tracks_service.clone();
        let (sender, receiver) = mpsc::channel();
        let (requests, duplicate_requests) = mpsc::channel();
        self.download_duplicate_requests = Some(duplicate_requests);
        let cancel = Arc::new(AtomicBool::new(false));
        self.download_progress = Some(receiver);
        self.download_cancel = Some(cancel.clone());
        let task = cx.background_executor().spawn(async move {
            let result = job(service, url.clone(), cancel, sender, requests);
            (url, result)
        });
        cx.spawn(async move |this, cx| {
            let (url, result) = task.await;
            let _ = this.update(cx, |app, cx| {
                app.active_download = None;
                app.active_download_collection = None;
                app.download_progress = None;
                app.download_cancel = None;
                app.download_duplicate_requests = None;
                if app
                    .duplicate_prompt
                    .as_ref()
                    .is_some_and(|p| p.is_download())
                {
                    app.duplicate_prompt = None;
                    if matches!(app.modal, Some(Modal::Duplicates)) {
                        app.modal = None;
                    }
                }
                app.job_progress = None;
                let (message, details) = match result {
                    Ok(report) => (report.message(), report.error_details()),
                    Err(error) => (
                        localized_format!("Не удалось скачать", "Download failed"),
                        Some(format!("{error:#}")),
                    ),
                };
                app.message = Some(message.clone());
                app.download_history.insert(
                    0,
                    DownloadResult {
                        url,
                        message,
                        details,
                    },
                );
                app.download_history.truncate(20);
                app.refresh(cx);
                app.start_next_download(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn download_options_view(&self, cx: &mut Context<Self>) -> AnyElement {
        div().flex().flex_col().gap_2()
            .child(self.duplicate_option_toggle("warn-download-duplicates", tr("Предупреждать о дубликатах"),
                &localized_format!("Показать найденные совпадения перед скачиванием.", "Show detected matches before downloading."),
                self.preferences.download_warn_duplicates)
                .on_click(cx.listener(|app, _, _, cx| {
                    app.preferences.download_warn_duplicates = !app.preferences.download_warn_duplicates;
                    app.save_preferences(); cx.notify();
                })))
            .child(self.duplicate_option_toggle("ignore-download-duplicates", tr("Игнорировать дубликаты"),
                tr("Использовать уже имеющиеся треки, не создавая новые копии."), self.preferences.download_ignore_duplicates)
                .on_click(cx.listener(|app, _, _, cx| {
                    app.preferences.download_ignore_duplicates = !app.preferences.download_ignore_duplicates;
                    app.save_preferences(); cx.notify();
                })))
            .child(div().text_xs().text_color(rgb(self.colors().muted)).child(
                if self.preferences.download_warn_duplicates {
                    localized_format!("При совпадении можно использовать имеющийся трек или скачать копию. Решение можно применить ко всем найденным дубликатам.",
                        "For matches, reuse the existing track or download a copy. Apply your choice to all detected duplicates if needed.")
                } else if self.preferences.download_ignore_duplicates {
                    tr("Совпадения будут пропущены без предупреждения. В плейлист добавятся уже имеющиеся треки.").into()
                } else {
                    localized_format!("Предупреждения выключены: найденные дубликаты будут скачаны как новые копии.",
                        "Warnings are off: detected duplicates will be downloaded as new copies.")
                }))
            .into_any_element()
    }

    pub(super) fn downloads_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let mut view = div()
            .id("downloads-scroll")
            .max_h(px(360.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3();
        if let Some(url) = &self.active_download {
            view = view.child(
                div()
                    .p_3()
                    .rounded_xl()
                    .bg(rgb(colors.active))
                    .child(
                        div().text_sm().child(
                            self.job_progress
                                .as_ref()
                                .map(|p| p.message())
                                .unwrap_or_default(),
                        ),
                    )
                    .child(
                        div()
                            .mt_2()
                            .text_xs()
                            .text_color(rgb(colors.muted))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(url.clone()),
                    )
                    .child(
                        self.button(
                            "stop-active-download",
                            &localized_format!("Остановить текущую", "Stop current"),
                            false,
                        )
                        .mt_2()
                        .on_click(cx.listener(|app, _, _, cx| {
                            if let Some(token) = &app.download_cancel {
                                token.store(true, Ordering::Relaxed);
                            }
                            cx.notify();
                        })),
                    ),
            );
        }
        for (index, request) in self.pending_downloads.iter().enumerate() {
            view = view.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(colors.button))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors.muted))
                            .child(format!("{:02}", index + 1)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(
                                request
                                    .collection
                                    .as_ref()
                                    .map(|c| c.collection.title.clone())
                                    .unwrap_or_else(|| request.url.clone()),
                            ),
                    )
                    .child(
                        self.icon_button(("remove-download", index as u64), "×", false)
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.pending_downloads.remove(index);
                                cx.notify();
                            })),
                    ),
            );
        }
        if !self.pending_downloads.is_empty() {
            view = view.child(
                self.button(
                    "clear-download-queue",
                    &localized_format!("Убрать все ожидающие", "Clear waiting downloads"),
                    false,
                )
                .on_click(cx.listener(|app, _, _, cx| {
                    app.pending_downloads.clear();
                    cx.notify();
                })),
            );
        }
        if self.active_download.is_none() && self.pending_downloads.is_empty() {
            view = view.child(div().text_sm().text_color(rgb(colors.muted)).child(
                localized_format!("Очередь загрузок пуста", "No pending downloads"),
            ));
        }
        for result in &self.download_history {
            view = view.child(
                div()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(colors.border))
                    .child(div().text_sm().child(result.message.clone()))
                    .child(
                        div()
                            .mt_2()
                            .text_xs()
                            .text_color(rgb(colors.muted))
                            .child(result.url.clone()),
                    )
                    .children(result.details.as_ref().map(|details| {
                        div()
                            .mt_2()
                            .text_xs()
                            .text_color(rgb(0xf1aaa0))
                            .child(details.clone())
                    })),
            );
        }
        view.into_any_element()
    }
}
