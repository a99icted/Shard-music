use super::*;
use shard::sync::{DuplicateDecision, DuplicateKind, DuplicateTrack, SyncOptions, SyncSelection};

pub(super) struct DuplicateRequest {
    tracks: Vec<DuplicateTrack>,
    response: mpsc::Sender<Option<Vec<DuplicateDecision>>>,
}
pub(super) struct DuplicatePrompt {
    request: DuplicateRequest,
    cursor: usize,
    decisions: Vec<DuplicateDecision>,
    apply_all: bool,
}
impl DuplicatePrompt {
    pub(super) fn new(request: DuplicateRequest) -> Self {
        Self {
            request,
            cursor: 0,
            decisions: vec![],
            apply_all: true,
        }
    }
}

impl MusicApp {
    pub(super) fn request_sync(
        &mut self,
        selection: SyncSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .sync_service
            .as_ref()
            .is_none_or(|service| service.is_transferring())
            || self.sync_duplicate_requests.is_some()
        {
            return;
        }
        if matches!(selection, SyncSelection::Track(_)) {
            self.begin_sync(selection, cx);
        } else {
            self.open_modal(Modal::SyncOptions(selection), window, cx);
        }
    }
    pub(super) fn sync_selection_title(&self, selection: SyncSelection) -> String {
        match selection {
            SyncSelection::Library => localized_format!(
                "Вся библиотека · треков: {}",
                "Entire library · {} tracks",
                self.tracks.len()
            ),
            SyncSelection::Playlist(id) => self
                .playlists
                .iter()
                .find(|p| p.id == id)
                .map(|p| {
                    localized_format!("{} · треков: {}", "{} · {} tracks", p.name, p.track_count)
                })
                .unwrap_or_default(),
            SyncSelection::Track(id) => self
                .tracks
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.title.clone())
                .unwrap_or_default(),
        }
    }
    fn begin_sync(&mut self, selection: SyncSelection, cx: &mut Context<Self>) {
        let Some(service) = self.sync_service.clone() else {
            return;
        };
        if service.is_transferring() || self.sync_duplicate_requests.is_some() {
            return;
        }
        let options = if matches!(selection, SyncSelection::Track(_)) {
            SyncOptions::default()
        } else {
            SyncOptions {
                warn_duplicates: self.preferences.sync_warn_duplicates,
                ignore_duplicates: self.preferences.sync_ignore_duplicates,
            }
        };
        let (requests, receiver) = mpsc::channel();
        self.sync_duplicate_requests = Some(receiver);
        self.sync_job_generation += 1;
        let generation = self.sync_job_generation;
        self.modal = None;
        self.error = None;
        self.message = None;
        let task = cx.background_executor().spawn(async move {
            let waiting_service = service.clone();
            service.send_selection(selection, options, move |duplicates| {
                let (response, receiver) = mpsc::channel();
                requests
                    .send(DuplicateRequest {
                        tracks: duplicates.to_vec(),
                        response,
                    })
                    .map_err(|_| anyhow::anyhow!(tr("Передача отменена")))?;
                loop {
                    match receiver.recv_timeout(Duration::from_millis(200)) {
                        Ok(Some(decisions)) => return Ok(decisions),
                        Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                            anyhow::bail!(tr("Передача отменена"))
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) if !waiting_service.is_enabled() => {
                            anyhow::bail!(tr("Передача отменена"))
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
            })
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                if app.sync_job_generation != generation {
                    return;
                }
                app.sync_duplicate_requests = None;
                app.sync_duplicate_prompt = None;
                if matches!(app.modal, Some(Modal::SyncDuplicates)) {
                    app.modal = None;
                }
                match result {
                    Ok(report) => {
                        app.message = Some(localized_format!(
                            "Передача завершена. Добавлено: {}. Без новых копий: {}.",
                            "Transfer complete. Added: {}. Reused: {}.",
                            report.copied,
                            report.reused
                        ))
                    }
                    Err(error) => app.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn cancel_sync_duplicates(&mut self) {
        if let Some(prompt) = self.sync_duplicate_prompt.take() {
            let _ = prompt.request.response.send(None);
        }
    }
    fn resolve_sync_duplicate(&mut self, copy: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.sync_duplicate_prompt.as_mut() else {
            return;
        };
        if prompt.apply_all {
            prompt
                .decisions
                .extend(prompt.request.tracks[prompt.cursor..].iter().map(|track| {
                    DuplicateDecision {
                        index: track.index,
                        copy,
                    }
                }));
            prompt.cursor = prompt.request.tracks.len();
        } else {
            prompt.decisions.push(DuplicateDecision {
                index: prompt.request.tracks[prompt.cursor].index,
                copy,
            });
            prompt.cursor += 1;
        }
        if prompt.cursor == prompt.request.tracks.len() {
            let prompt = self.sync_duplicate_prompt.take().unwrap();
            let _ = prompt.request.response.send(Some(prompt.decisions));
            self.modal = None;
            window.focus(&self.focus);
        }
        cx.notify();
    }
    fn sync_option_toggle(
        &self,
        id: &'static str,
        title: &str,
        description: &str,
        enabled: bool,
    ) -> Stateful<Div> {
        let colors = self.colors();
        div()
            .id(id)
            .p_3()
            .flex_shrink_0()
            .rounded_lg()
            .bg(rgb(colors.input))
            .border_1()
            .border_color(rgb(colors.border))
            .flex()
            .items_center()
            .gap_4()
            .cursor_pointer()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title.to_owned()),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_xs()
                            .text_color(rgb(colors.muted))
                            .child(description.to_owned()),
                    ),
            )
            .child(
                div()
                    .w(px(44.))
                    .h(px(26.))
                    .flex_shrink_0()
                    .rounded_full()
                    .p(px(3.))
                    .flex()
                    .items_center()
                    .bg(rgb(if enabled {
                        colors.accent
                    } else {
                        colors.border
                    }))
                    .when(enabled, |track| track.justify_end())
                    .child(div().size(px(20.)).rounded_full().bg(rgb(if enabled {
                        colors.bg
                    } else {
                        colors.text
                    }))),
            )
    }
    pub(super) fn sync_options_view(
        &self,
        selection: SyncSelection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = self.colors();
        div().flex().flex_col().gap_3().flex_shrink_0()
            .child(self.sync_option_toggle("warn-sync-duplicates", tr("Предупреждать о дубликатах"), tr("Показать найденные совпадения перед передачей."), self.preferences.sync_warn_duplicates)
                .on_click(cx.listener(|app, _, _, cx| { app.preferences.sync_warn_duplicates = !app.preferences.sync_warn_duplicates; app.save_preferences(); cx.notify(); })))
            .child(self.sync_option_toggle("ignore-sync-duplicates", tr("Игнорировать дубликаты"), tr("Использовать уже имеющиеся треки, не создавая новые копии."), self.preferences.sync_ignore_duplicates)
                .on_click(cx.listener(|app, _, _, cx| { app.preferences.sync_ignore_duplicates = !app.preferences.sync_ignore_duplicates; app.save_preferences(); cx.notify(); })))
            .child(div().text_xs().text_color(rgb(colors.muted)).child(if self.preferences.sync_warn_duplicates {
                tr("При совпадении вы сможете пропустить трек или передать копию. Решение можно применить ко всем совпадениям.")
            } else if self.preferences.sync_ignore_duplicates {
                tr("Совпадения будут пропущены без предупреждения. В плейлист добавятся уже имеющиеся треки.")
            } else {
                tr("Предупреждения выключены: найденные дубликаты будут переданы как новые копии.")
            }))
            .child(div().text_xs().text_color(rgb(colors.muted)).child(tr("Уже синхронизированные треки повторно не копируются. После обрыва повторите отправку — передача продолжится с сохранённого места.")))
            .child(self.button("confirm-sync", tr("Начать синхронизацию"), true).on_click(cx.listener(move |app, _, _, cx| app.begin_sync(selection, cx))))
            .child(self.button("cancel-sync-options", tr("Отмена"), false).on_click(cx.listener(|app, _, window, cx| app.dismiss(&Dismiss, window, cx))))
            .into_any_element()
    }
    pub(super) fn sync_options_help(&self) -> AnyElement {
        let colors = self.colors();
        div().flex().flex_col().gap_4().flex_shrink_0().text_sm()
            .child(div().child(div().font_weight(FontWeight::SEMIBOLD).child(tr("Предупреждать о дубликатах")))
                .child(div().mt_1().text_color(rgb(colors.muted)).child(tr("Если включено, передача остановится перед отправкой совпадений и спросит, что делать. Можно решить для каждого трека отдельно или включить «Применить ко всем найденным дубликатам»."))))
            .child(div().child(div().font_weight(FontWeight::SEMIBOLD).child(tr("Игнорировать дубликаты")))
                .child(div().mt_1().text_color(rgb(colors.muted)).child(tr("Если включено, дубликаты пропускаются: в плейлисте используется трек из библиотеки получателя. Если выключено, можно передать дополнительную копию. При включённых предупреждениях вы подтверждаете решение; без предупреждений оно применяется автоматически."))))
            .child(div().text_color(rgb(colors.muted)).child(tr("По умолчанию предупреждения включены, а игнорирование выключено. Само совпадение не запрещает отправку: вы всегда можете выбрать «Передать копию».")))
            .child(div().text_color(rgb(colors.muted)).child(tr("Совпадения проверяются по содержимому файла, ссылке на источник и метаданным. Уже синхронизированные треки обновляемого плейлиста переиспользуются автоматически. Обрыв связи не стирает полученные данные — повторная отправка продолжится с места остановки.")))
            .into_any_element()
    }
    pub(super) fn sync_duplicates_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let Some(prompt) = &self.sync_duplicate_prompt else {
            return div().into_any_element();
        };
        let track = &prompt.request.tracks[prompt.cursor];
        let mut view = div().flex().flex_col().gap_3().flex_shrink_0()
            .child(div().text_xs().text_color(rgb(colors.muted)).child(localized_format!("Совпадение {} из {}", "Match {} of {}", prompt.cursor + 1, prompt.request.tracks.len())))
            .child(div().p_3().rounded_lg().bg(rgb(colors.input)).text_sm()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(track.title.clone()))
                .child(div().mt_1().text_color(rgb(colors.muted)).child(track.artist.clone()))
                .child(div().mt_2().text_xs().text_color(rgb(colors.muted)).child(match track.kind {
                    DuplicateKind::IdenticalFile => tr("Найден файл с тем же содержимым."),
                    DuplicateKind::SameSource => tr("Найден трек с той же ссылкой на источник."),
                    DuplicateKind::SimilarMetadata => tr("Совпадают название, исполнитель и длительность. Аудиофайлы могут отличаться."),
                })))
            .child(self.sync_option_toggle("apply-all-duplicates", tr("Применить ко всем найденным дубликатам"), tr("Выключите, чтобы решить для каждого трека отдельно."), prompt.apply_all)
                .on_click(cx.listener(|app, _, _, cx| { if let Some(prompt) = app.sync_duplicate_prompt.as_mut() { prompt.apply_all = !prompt.apply_all; } cx.notify(); })));
        if prompt.apply_all && prompt.request.tracks.len() - prompt.cursor > 1 {
            view = view.child(
                div()
                    .id("duplicate-track-list")
                    .max_h(px(120.))
                    .overflow_y_scroll()
                    .text_xs()
                    .text_color(rgb(colors.muted))
                    .children(prompt.request.tracks[prompt.cursor..].iter().map(|track| {
                        div()
                            .py_1()
                            .child(format!("{} — {}", track.artist, track.title))
                    })),
            );
        }
        view.child(
            self.button(
                "copy-sync-duplicate",
                if prompt.apply_all {
                    tr("Передать копии")
                } else {
                    tr("Передать копию")
                },
                !self.preferences.sync_ignore_duplicates,
            )
            .on_click(
                cx.listener(|app, _, window, cx| app.resolve_sync_duplicate(true, window, cx)),
            ),
        )
        .child(
            self.button(
                "skip-sync-duplicate",
                if prompt.apply_all {
                    tr("Пропустить дубликаты")
                } else {
                    tr("Пропустить дубликат")
                },
                self.preferences.sync_ignore_duplicates,
            )
            .on_click(
                cx.listener(|app, _, window, cx| app.resolve_sync_duplicate(false, window, cx)),
            ),
        )
        .child(
            self.button("cancel-sync-duplicates", tr("Отменить передачу"), false)
                .on_click(cx.listener(|app, _, window, cx| app.dismiss(&Dismiss, window, cx))),
        )
        .into_any_element()
    }
    pub(super) fn toggle_favorite(&mut self, id: i64, cx: &mut Context<Self>) {
        if !self.favorite_pending.insert(id) {
            return;
        }
        let favorite = !self.favorite_ids.contains(&id);
        if favorite {
            self.favorite_ids.insert(id);
        } else {
            self.favorite_ids.remove(&id);
        }
        let service = self.favorites_service.clone();
        let task = cx
            .background_executor()
            .spawn(async move { service.set(id, favorite) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                app.favorite_pending.remove(&id);
                if let Err(error) = result {
                    app.error = Some(format!("{error:#}"));
                }
                app.refresh(cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn start_sync(&mut self, cx: &mut Context<Self>) {
        if self.sync_service.is_some() {
            return;
        }
        let result = Database::open(&self.data_directory).and_then(|db| {
            let service = shard::sync::SyncService::start(
                db,
                self.preferences.sync_device_id.clone(),
                self.preferences.sync_device_name.clone(),
            )?;
            let qr = self
                .data_directory
                .join(format!("sync-qr-{}.png", uuid::Uuid::new_v4()));
            if let Err(error) = service.qr_image(&qr) {
                service.stop();
                return Err(error);
            }
            Ok((service, qr))
        });
        match result {
            Ok((service, qr)) => {
                self.sync_service = Some(service);
                self.sync_qr = Some(qr);
                self.sync_status = Default::default();
                self.preferences.sync_enabled = true;
            }
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                self.preferences.sync_enabled = false;
            }
        }
        self.save_preferences();
        cx.notify();
    }
    fn toggle_sync(&mut self, cx: &mut Context<Self>) {
        if let Some(service) = self.sync_service.take() {
            service.stop();
            self.cancel_sync_duplicates();
            self.sync_duplicate_requests = None;
            self.sync_job_generation += 1;
            if matches!(self.modal, Some(Modal::SyncDuplicates)) {
                self.modal = None;
            }
            self.preferences.sync_enabled = false;
            self.sync_status = Default::default();
            if let Some(path) = self.sync_qr.take() {
                let _ = std::fs::remove_file(path);
            }
            self.save_preferences();
            cx.notify();
        } else {
            self.start_sync(cx);
        }
    }
    fn pair_sync(&mut self, cx: &mut Context<Self>) {
        let Some(service) = self.sync_service.clone() else {
            return;
        };
        let code = self.sync_input.read(cx).value().to_owned();
        let generation = self.sync_job_generation;
        let task = cx
            .background_executor()
            .spawn(async move { service.pair(&code) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                if app.sync_job_generation != generation || app.sync_service.is_none() {
                    return;
                }
                match result {
                    Ok(()) => app.message = Some(tr("Устройства подключены").into()),
                    Err(error) => app.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn sync_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let enabled = self.sync_service.is_some();
        let mut view = div().p_4().rounded_xl().bg(self.surface(colors.panel)).border_1().border_color(rgb(colors.border)).flex().flex_col().gap_4()
            .child(
                div().flex().items_start().justify_between().gap_3()
                    .child(div().flex_1().min_w_0().text_lg().font_weight(FontWeight::SEMIBOLD).child(tr("Синхронизация по локальной сети")))
                    .child(
                        div().id("sync-help").size_7().flex_shrink_0().rounded_full().border_1().border_color(rgb(colors.muted))
                            .flex().items_center().justify_center().text_sm().font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(colors.muted)).cursor_pointer()
                            .hover(move |style| style.bg(rgb(colors.button_hover)).text_color(rgb(colors.accent)).border_color(rgb(colors.accent)))
                            .child("?")
                            .on_click(cx.listener(|app, _, window, cx| app.open_modal(Modal::SyncHelp, window, cx))),
                    ),
            )
            .child(div().text_sm().text_color(rgb(colors.muted)).child(tr("Подключите два устройства к одной сети. Передавайте выбранные треки и плейлисты через меню ⋯. Музыка сохраняется на принимающем устройстве.")))
            .child(
                div().id("toggle-sync").flex().items_center().justify_between().gap_4().py_2().cursor_pointer()
                    .child(
                        div().flex_1().min_w_0().text_sm().child(tr("Синхронизация"))
                            .child(div().mt_1().text_xs().text_color(rgb(colors.muted)).child(if enabled { tr("Включена") } else { tr("Выключена") })),
                    )
                    .child(
                        div().w(px(44.)).h(px(26.)).flex_shrink_0().rounded_full().p(px(3.)).flex().items_center()
                            .justify_between().bg(rgb(if enabled { colors.accent } else { colors.border }))
                            .when(enabled, |track| track.justify_end())
                            .hover(move |style| style.bg(rgb(if enabled { colors.accent } else { colors.button_hover })))
                            .child(div().size(px(20.)).rounded_full().bg(rgb(if enabled { colors.bg } else { colors.text }))),
                    )
                    .on_click(cx.listener(|app, _, _, cx| app.toggle_sync(cx))),
            );
        if let Some(service) = &self.sync_service {
            view = view
                .child(self.button("sync-all-tracks", tr("Синхронизировать все треки"), false).on_click(cx.listener(|app, _, window, cx| app.request_sync(SyncSelection::Library, window, cx))))
                .child(div().text_sm().child(format!("{}: {}", tr("Это устройство"), self.preferences.sync_device_name)))
                .child(div().text_xs().text_color(rgb(colors.muted)).child(tr("Код подключения")))
                .child(
                    div().w_full().min_w_0().flex().items_center().gap_3().px_3().py_2().rounded_lg()
                        .bg(rgb(colors.input)).border_1().border_color(rgb(colors.border))
                        .child(div().flex_1().min_w_0().text_sm().text_color(rgb(colors.text)).overflow_hidden().text_ellipsis().child(service.code().to_owned()))
                        .child(
                            div().id("copy-sync-code").size_8().flex_shrink_0().rounded_lg().flex().items_center().justify_center().cursor_pointer()
                                .hover(move |style| style.bg(rgb(colors.button_hover)))
                                .child(svg().path("icons/copy.svg").size_5().text_color(rgb(colors.accent)))
                                .on_click(cx.listener(|app, _, _, cx| {
                                    if let Some(service) = &app.sync_service {
                                        cx.write_to_clipboard(ClipboardItem::new_string(service.code().to_owned()));
                                        app.message = Some(tr("Код подключения скопирован").into());
                                        cx.notify();
                                    }
                                })),
                        ),
                )
                .children(self.sync_qr.as_ref().map(|path| div().p_3().bg(rgb(0xffffff)).rounded_lg().w(px(224.)).child(img(path.clone()).size(px(200.)))))
                .child(div().text_xs().text_color(rgb(colors.muted)).child(tr("Сканируйте QR в Shard на телефоне. Подключение работает через роутер или точку доступа телефона/ПК. После смены сети включите синхронизацию заново и отсканируйте новый QR.")))
                .child(self.sync_input.clone())
                .child(self.button("pair-sync", tr("Подключить устройство"), true).on_click(cx.listener(|app, _, _, cx| app.pair_sync(cx))))
                .child(div().text_sm().text_color(rgb(colors.accent)).child(self.sync_status.peer.as_ref().map(|peer| format!("{}: {peer}", tr("Подключено"))).unwrap_or_else(|| tr("Ожидание подключения устройства").into())))
                .child(div().text_xs().text_color(rgb(colors.muted)).child(tr("Передача запускается вручную. Полученные треки добавляются без удаления музыки на другом устройстве. Для подключения может потребоваться разрешить Shard в брандмауэре Windows для частной сети.")));
        }
        if let Some(error) = &self.sync_status.error {
            view = view.child(
                div()
                    .text_sm()
                    .text_color(rgb(0xf1aaa0))
                    .child(error.clone()),
            );
        }
        view.into_any_element()
    }

    pub(super) fn sync_help(&self) -> AnyElement {
        let colors = self.colors();
        div().flex().flex_col().gap_4().flex_shrink_0()
            .children([
                ("1", tr("Включите синхронизацию"), tr("На обоих устройствах откройте «Настройки → Синхронизация» и включите тумблер. Устройства должны быть подключены к одной сети Wi-Fi или локальной сети.")),
                ("2", tr("Соедините устройства"), tr("Нажмите иконку копирования справа от кода подключения на одном устройстве. Вставьте код в поле на другом устройстве и нажмите «Подключить устройство». Достаточно ввести код на одном из двух клиентов.")),
                ("3", tr("Выберите музыку для передачи"), tr("Откройте меню ⋯ у трека или в шапке плейлиста и выберите «Синхронизировать трек» или «Синхронизировать плейлист». Так можно передать и «Избранное». Прогресс передачи отображается на обоих устройствах.")),
                ("4", tr("Слушайте на другом устройстве"), tr("Аудиофайлы сохранятся в библиотеке получателя вместе с названиями, исполнителями и отметками избранного. При повторной отправке плейлиста добавятся новые треки. Для других совпадений можно выбрать пропуск или дополнительную копию. После обрыва повторите отправку, чтобы продолжить. Удаление музыки на одном устройстве не удаляет её на другом.")),
            ].into_iter().map(|(number, title, text)| {
                div().flex().items_start().gap_3().flex_shrink_0()
                    .child(div().size_6().flex_shrink_0().rounded_full().bg(rgb(colors.button)).text_color(rgb(colors.accent)).text_xs().flex().items_center().justify_center().child(number))
                    .child(div().flex_1().min_w_0().text_sm().child(div().font_weight(FontWeight::SEMIBOLD).child(title)).child(div().mt_1().text_color(rgb(colors.muted)).child(text)))
            }))
            .child(div().text_xs().text_color(rgb(colors.muted)).child(tr("QR содержит локальные адреса устройства. После смены сети или включения точки доступа перезапустите синхронизацию и отсканируйте новый QR. Обложки, фоны и настройки пока не передаются.")))
            .into_any_element()
    }
    pub(super) fn sync_progress_card(&self) -> Option<AnyElement> {
        let colors = self.colors();
        let progress = self.sync_status.progress.as_ref()?;
        let fraction = if progress.complete {
            1.
        } else if progress.total > 0 {
            progress.bytes as f32 / progress.total as f32
        } else {
            0.
        };
        Some(
            div()
                .flex_shrink_0()
                .p_3()
                .rounded_lg()
                .bg(rgb(colors.input))
                .border_1()
                .border_color(rgb(colors.border))
                .child(div().text_xs().text_color(rgb(colors.accent)).child(
                    if self.sync_status.error.is_some() {
                        tr("Ошибка синхронизации")
                    } else if progress.complete {
                        tr("Передача завершена")
                    } else if progress.receiving {
                        tr("Получение музыки")
                    } else {
                        tr("Отправка музыки")
                    },
                ))
                .child(
                    div()
                        .my_2()
                        .text_xs()
                        .text_ellipsis()
                        .overflow_hidden()
                        .child(progress.title.clone()),
                )
                .child(
                    div()
                        .h(px(4.))
                        .w_full()
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
                .child(div().mt_2().text_xs().text_color(rgb(colors.muted)).child(
                    if progress.total > 0 {
                        format!(
                            "{:.0}% · {:.1} / {:.1} MB",
                            fraction * 100.,
                            progress.bytes as f64 / 1048576.,
                            progress.total as f64 / 1048576.
                        )
                    } else if progress.complete {
                        "100%".into()
                    } else {
                        tr("Подготовка передачи…").into()
                    },
                ))
                .into_any_element(),
        )
    }
}
