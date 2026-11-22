use super::*;

impl MusicApp {
    pub(super) fn connected_devices_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let mut view = div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_4()
            .p_4()
            .rounded_xl()
            .bg(self.surface(colors.panel))
            .border_1()
            .border_color(rgb(colors.border))
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr("Подключённые устройства")),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(colors.muted))
                    .child(tr("Это устройство")),
            )
            .child(
                div()
                    .text_sm()
                    .child(self.preferences.sync_device_name.clone()),
            );
        let Some(service) = &self.sync_service else {
            return view
                .child(div().text_sm().child(tr("Синхронизация выключена")))
                .into_any_element();
        };
        let status = service.status();
        if let Some(device) = status.paired_device {
            view = view.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(colors.input))
                    .border_1()
                    .border_color(rgb(colors.border))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(device.name),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(colors.accent))
                            .child(tr("Сопряжено в текущем сеансе")),
                    )
                    .child(div().text_sm().child(localized_format!(
                        "Адрес: {}",
                        "Address: {}",
                        device.address
                    )))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors.muted))
                            .child(format!("ID: {}", device.device_id)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors.muted))
                            .child(localized_format!(
                                "Протокол: {} · ручная передача",
                                "Protocol: {} · manual transfer",
                                device.protocol_version
                            )),
                    )
                    .child(
                        self.button(
                            "devices-send-library",
                            tr("Синхронизировать все треки"),
                            false,
                        )
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.request_sync(shard::sync::SyncSelection::Library, window, cx)
                        })),
                    ),
            );
            view = view.children(self.sync_progress_card());
        } else {
            view = view.child(div().py_4().text_sm().text_color(rgb(colors.muted))
                .child(tr("Пока нет подключённых устройств. Подключите другое устройство по коду в разделе «Синхронизация».")));
        }
        view
            .child(div().text_sm().text_color(rgb(colors.muted)).child(tr("Сейчас поддерживается одно сопряжённое устройство. Постоянное соединение и live-управление ещё не включены; доступность проверяется при передаче. После перезапуска синхронизации подключитесь заново.")))
            .child(self.button("devices-open-sync", tr("Настроить подключение"), false)
                .on_click(cx.listener(|app, _, _, cx| {
                    app.settings_category = super::settings::SettingsCategory::Synchronization;
                    cx.notify();
                })))
            .into_any_element()
    }
}
