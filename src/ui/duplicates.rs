use super::*;
use shard::sync::{DuplicateDecision, DuplicateKind, DuplicateTrack};

pub(super) struct DuplicateRequest {
    pub(super) tracks: Vec<DuplicateTrack>,
    pub(super) response: mpsc::Sender<Option<Vec<DuplicateDecision>>>,
    pub(super) download: bool,
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
    pub(super) fn is_download(&self) -> bool {
        self.request.download
    }
}

impl MusicApp {
    pub(super) fn cancel_duplicate_review(&mut self) {
        if let Some(prompt) = self.duplicate_prompt.take() {
            if prompt.is_download() {
                if let Some(cancel) = &self.download_cancel {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            let _ = prompt.request.response.send(None);
        }
    }
    fn resolve_duplicate(&mut self, copy: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.duplicate_prompt.as_mut() else {
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
            let prompt = self.duplicate_prompt.take().unwrap();
            let _ = prompt.request.response.send(Some(prompt.decisions));
            self.modal = None;
            window.focus(&self.focus);
        }
        cx.notify();
    }
    pub(super) fn duplicate_option_toggle(
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
    pub(super) fn duplicates_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let Some(prompt) = &self.duplicate_prompt else {
            return div().into_any_element();
        };
        let track = &prompt.request.tracks[prompt.cursor];
        let download = prompt.is_download();
        let mut view = div().flex().flex_col().gap_3().flex_shrink_0()
            .child(div().text_xs().text_color(rgb(colors.muted)).child(localized_format!("Совпадение {} из {}", "Match {} of {}", prompt.cursor + 1, prompt.request.tracks.len())))
            .child(div().p_3().rounded_lg().bg(rgb(colors.input)).text_sm()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(track.title.clone()))
                .child(div().mt_1().text_color(rgb(colors.muted)).child(track.artist.clone()))
                .child(div().mt_1().text_xs().text_color(rgb(colors.muted)).child(localized_format!(
                    "Совпадает с: {}", "Matches: {}", track.existing_title)))
                .child(div().mt_2().text_xs().text_color(rgb(colors.muted)).child(match track.kind {
                    DuplicateKind::IdenticalFile => tr("Найден файл с тем же содержимым."),
                    DuplicateKind::SameSource => tr("Найден трек с той же ссылкой на источник."),
                    DuplicateKind::SimilarMetadata => tr("Совпадают название, исполнитель и длительность. Аудиофайлы могут отличаться."),
                })))
            .child(self.duplicate_option_toggle("apply-all-duplicates", tr("Применить ко всем найденным дубликатам"), tr("Выключите, чтобы решить для каждого трека отдельно."), prompt.apply_all)
                .on_click(cx.listener(|app, _, _, cx| { if let Some(prompt) = app.duplicate_prompt.as_mut() { prompt.apply_all = !prompt.apply_all; } cx.notify(); })));
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
                    if download {
                        localized_format!("Скачать копии", "Download copies")
                    } else {
                        tr("Передать копии").into()
                    }
                } else {
                    if download {
                        localized_format!("Скачать копию", "Download copy")
                    } else {
                        tr("Передать копию").into()
                    }
                }
                .as_str(),
                !(if download {
                    self.preferences.download_ignore_duplicates
                } else {
                    self.preferences.sync_ignore_duplicates
                }),
            )
            .on_click(cx.listener(|app, _, window, cx| app.resolve_duplicate(true, window, cx))),
        )
        .child(
            self.button(
                "skip-sync-duplicate",
                if prompt.apply_all {
                    tr("Пропустить дубликаты")
                } else {
                    tr("Пропустить дубликат")
                },
                if download {
                    self.preferences.download_ignore_duplicates
                } else {
                    self.preferences.sync_ignore_duplicates
                },
            )
            .on_click(cx.listener(|app, _, window, cx| app.resolve_duplicate(false, window, cx))),
        )
        .child(
            self.button(
                "cancel-sync-duplicates",
                if download {
                    tr("Остановить загрузку")
                } else {
                    tr("Отменить передачу")
                },
                false,
            )
            .on_click(cx.listener(|app, _, window, cx| app.dismiss(&Dismiss, window, cx))),
        )
        .into_any_element()
    }
}
